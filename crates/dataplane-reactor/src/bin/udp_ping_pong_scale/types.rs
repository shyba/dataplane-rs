#[derive(Clone, Copy, Debug)]
pub(crate) enum Backend {
    Uring,
    Syscall,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ServerRecvMode {
    From,
    Mmsg,
    MsgMultishot,
}
