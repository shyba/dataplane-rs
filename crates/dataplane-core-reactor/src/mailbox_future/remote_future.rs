use crate::mailbox_future::remote_task::RemoteTaskReceiver;
use crate::mailbox_future::result_cell::{RemoteTaskRecvError, RESULT_CLOSED};
use crate::mailbox_future::signal::RemoteSignalHandle;
use crate::runtime_tls::{current_shared_wake, park_current_signal};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};

impl<T: Send + 'static> RemoteTaskReceiver<T> {
    #[inline(always)]
    pub fn try_recv(&self) -> Option<Result<T, RemoteTaskRecvError>> {
        if let Some(value) = self.result.take_ready() {
            return Some(Ok(value));
        }
        if self.result.state.load(Ordering::Acquire) == RESULT_CLOSED {
            self.result.receiver_open.store(false, Ordering::Release);
            return Some(Err(RemoteTaskRecvError));
        }
        None
    }

    #[inline(always)]
    pub fn recv(&self) -> Result<T, RemoteTaskRecvError> {
        let Some(owner_shard) = self.owner_shard else {
            return Err(RemoteTaskRecvError);
        };
        loop {
            if let Some(result) = self.try_recv() {
                return result;
            }
            let seen = self.global.shard_signal_epoch(owner_shard);
            if let Some(result) = self.try_recv() {
                return result;
            }
            self.global.wait_shard_signal(owner_shard, seen);
            #[cfg(not(target_os = "linux"))]
            std::hint::spin_loop();
        }
    }
}

impl<T: Send + 'static> Future for RemoteTaskReceiver<T> {
    type Output = Result<T, RemoteTaskRecvError>;

    #[inline(always)]
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(result) = self.try_recv() {
            return Poll::Ready(result);
        }
        let Some(signal_key) = self.signal else {
            return Poll::Ready(Err(RemoteTaskRecvError));
        };
        let signal_handle = RemoteSignalHandle::new(self.global.clone(), signal_key);
        if park_current_signal(signal_handle) {
            return Poll::Pending;
        }
        {
            if let Some(signal) = self.global.signal_entry(signal_key) {
                if let Some(wake) = signal.register(current_shared_wake()) {
                    wake.wake();
                }
            }
        }
        match self.try_recv() {
            Some(result) => Poll::Ready(result),
            None => Poll::Pending,
        }
    }
}

impl<T: Send + 'static> Drop for RemoteTaskReceiver<T> {
    fn drop(&mut self) {
        self.result.receiver_open.store(false, Ordering::Release);
        self.result.close();
        if let Some(signal_key) = self.signal {
            if let Some(signal) = self.global.signal_entry(signal_key) {
                signal.clear();
            }
            self.global.unregister_signal(signal_key);
        }
    }
}
