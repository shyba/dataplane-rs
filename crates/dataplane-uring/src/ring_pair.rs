use io_uring::IoUring;

#[allow(dead_code)]
pub struct RingPairBorrow<'a> {
    pub latency: &'a IoUring,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub main: &'a IoUring,
}

#[allow(dead_code)]
pub struct RingPairBorrowMut<'a> {
    pub latency: &'a mut IoUring,
    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub main: &'a mut IoUring,
}
