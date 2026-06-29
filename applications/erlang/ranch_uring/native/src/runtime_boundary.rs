use std::os::fd::RawFd;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::errors::{NifError, Result};
use crate::runtime_ingress::{ingress_send, IngressReceiver, IngressSender};

/// Shared shard-boundary plumbing for command publication and wakeup signaling.
///
/// The wakeup contract is intentionally narrow:
/// - command publication goes through `IngressSender`
/// - `pending_commands` tracks whether the shard has already been notified
/// - the first publication transition from 0 -> N triggers a single `eventfd` wakeup
/// - the shard-side consumer drains the `eventfd` and re-arms polling after wakeup
///
/// This keeps the M7 hosted-step migration aligned with the existing runtime
/// boundary without changing the wakeup semantics.
#[repr(align(64))]
pub(super) struct ShardControl {
    pub(super) pending_commands: AtomicUsize,
    pub(super) next_request_id: AtomicU64,
}

impl ShardControl {
    pub(super) fn new() -> Self {
        Self {
            pending_commands: AtomicUsize::new(0),
            next_request_id: AtomicU64::new(1),
        }
    }
}

pub(super) struct ShardSender<C> {
    pub(super) tx: IngressSender<C>,
    pub(super) wakeup_fd: RawFd,
    pub(super) control: Arc<ShardControl>,
}

impl<C> ShardSender<C> {
    pub(super) fn send(&self, cmd: C) -> Result<()> {
        let len = 1usize;
        ingress_send(&self.tx, vec![cmd]).map_err(|_| NifError::Closed)?;
        self.after_publish(len)
    }

    pub(super) fn send_many(&self, mut cmds: Vec<C>) -> Result<()> {
        self.send_many_staged(&mut cmds)
    }

    pub(super) fn send_many_staged(&self, cmds: &mut Vec<C>) -> Result<()> {
        let len = cmds.len();
        if len == 0 {
            return Ok(());
        }
        let batch = std::mem::take(cmds);
        ingress_send(&self.tx, batch).map_err(|_| NifError::Closed)?;
        self.after_publish(len)
    }

    fn after_publish(&self, len: usize) -> Result<()> {
        if self
            .control
            .pending_commands
            .fetch_add(len, Ordering::AcqRel)
            == 0
        {
            let ret = loop {
                let val: u64 = 1;
                // SAFETY: eventfd write uses a pointer to a stack-local 8-byte counter with
                // matching length. EINTR is retried and other errors are surfaced.
                let ret = unsafe {
                    libc::write(self.wakeup_fd, &val as *const u64 as *const libc::c_void, 8)
                };
                if ret < 0 {
                    let errno = std::io::Error::last_os_error();
                    if errno.raw_os_error() == Some(libc::EINTR) {
                        continue;
                    }
                    return Err(NifError::from_errno(
                        errno.raw_os_error().unwrap_or(libc::EIO),
                    ));
                } else {
                    break Ok(());
                }
            };
            if let Err(err) = ret {
                self.control
                    .pending_commands
                    .fetch_sub(len, Ordering::AcqRel);
                return Err(err);
            }
        }
        Ok(())
    }
}

pub(super) type ShardReceiver<C> = IngressReceiver<C>;
