pub trait BalancedHostPolicy {
    fn before_wait_until(&mut self, deadline_ns: u64);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BalancedRecordingHostPolicy {
    last_deadline_ns: Option<u64>,
    wait_requests: usize,
}

impl BalancedRecordingHostPolicy {
    #[inline]
    pub fn last_deadline_ns(&self) -> Option<u64> {
        self.last_deadline_ns
    }

    #[inline]
    pub fn wait_requests(&self) -> usize {
        self.wait_requests
    }
}

impl BalancedHostPolicy for BalancedRecordingHostPolicy {
    #[inline]
    fn before_wait_until(&mut self, deadline_ns: u64) {
        self.last_deadline_ns = Some(deadline_ns);
        self.wait_requests = self.wait_requests.saturating_add(1);
    }
}
