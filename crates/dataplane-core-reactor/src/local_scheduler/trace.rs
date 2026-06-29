pub const TRACE_STAMP_DEPTH: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceStep {
    Ingress,
    Queued,
    Dispatch,
    Executed,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceStamp {
    pub step: TraceStep,
    pub ticks: u64,
}

impl Default for TraceStamp {
    fn default() -> Self {
        Self {
            step: TraceStep::Ingress,
            ticks: 0,
        }
    }
}

#[cfg(feature = "trace-stamps")]
#[derive(Clone, Debug)]
pub(crate) struct TraceRing {
    stamps: [TraceStamp; TRACE_STAMP_DEPTH],
    next: usize,
    len: usize,
}

#[cfg(feature = "trace-stamps")]
impl Default for TraceRing {
    fn default() -> Self {
        Self {
            stamps: [TraceStamp::default(); TRACE_STAMP_DEPTH],
            next: 0,
            len: 0,
        }
    }
}

#[cfg(feature = "trace-stamps")]
impl TraceRing {
    #[inline(always)]
    pub(crate) fn push(&mut self, step: TraceStep, ticks: u64) {
        self.stamps[self.next] = TraceStamp { step, ticks };
        self.next = (self.next + 1) % TRACE_STAMP_DEPTH;
        if self.len < TRACE_STAMP_DEPTH {
            self.len += 1;
        }
    }

    #[inline(always)]
    pub(crate) fn snapshot(&self) -> [TraceStamp; TRACE_STAMP_DEPTH] {
        if self.len == 0 {
            return [TraceStamp::default(); TRACE_STAMP_DEPTH];
        }
        let mut out = [TraceStamp::default(); TRACE_STAMP_DEPTH];
        let start = if self.len < TRACE_STAMP_DEPTH {
            0
        } else {
            self.next
        };
        let mut i = 0usize;
        while i < self.len {
            out[i] = self.stamps[(start + i) % TRACE_STAMP_DEPTH];
            i += 1;
        }
        out
    }
}
