use io_uring::IoUring;

#[allow(dead_code)]
pub(super) struct RingPairBorrow<'a> {
    pub(super) latency: &'a IoUring,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) main: &'a IoUring,
}

#[allow(dead_code)]
pub(super) struct RingPairBorrowMut<'a> {
    pub(super) latency: &'a mut IoUring,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) main: &'a mut IoUring,
}
