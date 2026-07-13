use std::os::fd::RawFd;

use io_uring::{cqueue, types};

use crate::reactor::UdpRecvSlot;

#[derive(Debug)]
pub(super) struct UdpBatchState {
    pub(super) fd: RawFd,
    pub(super) slots_ptr: *mut UdpRecvSlot,
    pub(super) slots_len: usize,
    pub(super) flags: i32,
    pub(super) next_slot: usize,
    pub(super) filled: usize,
    /// Set after ENOBUFS so the next Multi re-arm re-issues ProvideBuffers
    /// even mid-batch (the kernel buffer ring is exhausted).
    pub(super) replenish_buffers: bool,
    pub(super) mode: UdpBatchMode,
}

#[derive(Debug)]
pub(super) enum UdpBatchMode {
    Single {
        iovecs: Box<[libc::iovec]>,
        msgs: Box<[libc::msghdr]>,
    },
    Multi {
        provided: Box<[u8]>,
        buf_len: usize,
        bgid: u16,
        nbufs: u16,
        msg_template: libc::msghdr,
    },
}

pub(super) fn fill_udp_batch_slot_from_multishot(
    state: &mut UdpBatchState,
    result: i32,
    flags: u32,
) -> std::io::Result<()> {
    let UdpBatchMode::Multi {
        provided,
        buf_len,
        msg_template,
        ..
    } = &mut state.mode
    else {
        return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
    };
    let bid = cqueue::buffer_select(flags)
        .ok_or_else(|| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    let offset = (bid as usize) * (*buf_len);
    if offset + *buf_len > provided.len() {
        return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
    }
    let raw_buf = &provided[offset..offset + *buf_len];
    let parsed = types::RecvMsgOut::parse(raw_buf, msg_template)
        .map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    if state.slots_ptr.is_null() || state.next_slot >= state.slots_len {
        return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
    }
    // SAFETY: slots_ptr is non-null and slots_len > 0 (enforced above).
    let slots = unsafe { std::slice::from_raw_parts_mut(state.slots_ptr, state.slots_len) };
    let slot = &mut slots[state.next_slot];
    let payload = parsed.payload_data();
    let copy_len = payload.len().min(slot.buf_len).min(result.max(0) as usize);
    if copy_len > 0 && !slot.buf_ptr.is_null() {
        // SAFETY: payload is valid for copy_len bytes; slot.buf_ptr is valid and non-null
        // (checked above). copy_len is bounded by slot.buf_len and result, so the copy
        // is within both buffers.
        unsafe {
            std::ptr::copy_nonoverlapping(payload.as_ptr(), slot.buf_ptr, copy_len);
        }
    }
    slot.recv_len = copy_len;
    let name = parsed.name_data();
    let addr_copy_len = name
        .len()
        .min(std::mem::size_of::<libc::sockaddr_storage>());
    if addr_copy_len > 0 {
        // SAFETY: name is valid for addr_copy_len bytes; slot.addr is valid.
        unsafe {
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                (&mut slot.addr as *mut libc::sockaddr_storage).cast::<u8>(),
                addr_copy_len,
            );
        }
    }
    slot.addr_len = addr_copy_len as libc::socklen_t;
    state.filled += 1;
    state.next_slot += 1;
    Ok(())
}
