use crate::embedded_host_loop::EmbeddedHostAdapter;

const FAKE_HOST_IDLE_TICK_BUMP: usize = 100;

pub(crate) struct FakeHost {
    pub(crate) now_ns: u64,
    pub(crate) ticks: usize,
}

#[derive(Default)]
pub(crate) struct CountingHost {
    pub(crate) now_ns: u64,
    pub(crate) now_calls: usize,
    pub(crate) idle_calls: usize,
}

#[derive(Default)]
pub(crate) struct AdvancingHost {
    pub(crate) next_now_ns: u64,
    pub(crate) step_ns: u64,
    pub(crate) seen_now_ns: Vec<u64>,
    pub(crate) idle_calls: usize,
}

#[derive(Default)]
pub(crate) struct IdleRecordingHost {
    pub(crate) now_ns: u64,
    pub(crate) now_calls: usize,
    pub(crate) idle_notifications: usize,
}

#[derive(Default)]
pub(crate) struct NowOnlyHost {
    pub(crate) now_ns: u64,
    pub(crate) now_calls: usize,
}

pub(crate) struct NowOnlyTraitBoundHost;

impl EmbeddedHostAdapter for FakeHost {
    fn now_ns(&mut self) -> u64 {
        self.ticks += 1;
        self.now_ns
    }

    fn on_idle(&mut self) {
        self.ticks += FAKE_HOST_IDLE_TICK_BUMP;
    }
}

impl EmbeddedHostAdapter for CountingHost {
    fn now_ns(&mut self) -> u64 {
        self.now_calls += 1;
        self.now_ns
    }

    fn on_idle(&mut self) {
        self.idle_calls += 1;
    }
}

impl EmbeddedHostAdapter for AdvancingHost {
    fn now_ns(&mut self) -> u64 {
        let now_ns = self.next_now_ns;
        self.seen_now_ns.push(now_ns);
        self.next_now_ns = self.next_now_ns.saturating_add(self.step_ns);
        now_ns
    }

    fn on_idle(&mut self) {
        self.idle_calls += 1;
    }
}

impl EmbeddedHostAdapter for IdleRecordingHost {
    fn now_ns(&mut self) -> u64 {
        self.now_calls += 1;
        self.now_ns
    }

    fn on_idle(&mut self) {
        self.idle_notifications += 1;
    }
}

impl EmbeddedHostAdapter for NowOnlyHost {
    fn now_ns(&mut self) -> u64 {
        self.now_calls += 1;
        self.now_ns
    }
}

impl EmbeddedHostAdapter for NowOnlyTraitBoundHost {
    fn now_ns(&mut self) -> u64 {
        0
    }
}
