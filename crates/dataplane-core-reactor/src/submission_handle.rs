use crate::wake_handle::WakeHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmissionHandle<Token> {
    token: Token,
    wake: WakeHandle,
}

impl<Token: Copy> SubmissionHandle<Token> {
    #[inline(always)]
    pub fn new(token: Token, wake: WakeHandle) -> Self {
        Self { token, wake }
    }

    #[inline(always)]
    pub fn token(&self) -> Token {
        self.token
    }

    #[inline(always)]
    pub fn wake_handle(&self) -> WakeHandle {
        self.wake
    }
}
