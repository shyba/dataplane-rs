//! Fixed-iteration probe for instruction-count comparison of the
//! tick_completions_or_wait hot path (idle and ready-completions cases).
use std::hint::black_box;

use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, StepResult};
use dataplane_core_reactor::reactor_driver::{
    DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait,
};
use dataplane_core_reactor::reactor_model::{NetEvent, NetOp, NetOpKind, OpToken};
use dataplane_runtime::runtime_profiles::build_balanced_runtime;

struct EmptyTask;
impl NativeTask for EmptyTask {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        StepResult::Complete
    }
}

#[derive(Default)]
struct ProbeDriver {
    emit_event: bool,
}

impl ReactorDriver for ProbeDriver {
    type Error = std::io::Error;
    type Token = OpToken;
    type Submit = NetOp;
    type Event = NetEvent;
    fn submit(&mut self, _op: NetOp, _token: OpToken) -> Result<(), Self::Error> {
        Ok(())
    }
    fn flush(&mut self) -> Result<usize, Self::Error> {
        Ok(0)
    }
    fn drain<F>(&mut self, _max: usize, mut on_event: F) -> Result<usize, Self::Error>
    where
        F: FnMut(NetEvent),
    {
        if self.emit_event {
            on_event(NetEvent::OpComplete {
                token: OpToken(1),
                kind: NetOpKind::Recv,
                result: 1,
                flags: 0,
            });
            Ok(1)
        } else {
            Ok(0)
        }
    }
    fn outstanding(&self) -> usize {
        usize::from(self.emit_event)
    }
    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities {
            backend: DriverBackendKind::Syscall,
            supports_accept_multi: false,
            supports_multishot: false,
            supports_fixed_buffers: false,
            supports_sqpoll: false,
        }
    }
}

impl ReactorDriverWait for ProbeDriver {
    type Error = std::io::Error;
    type Readiness = ();
    fn readiness(&self) -> Option<()> {
        None
    }
    fn wait(&mut self, _min: usize) -> Result<usize, Self::Error> {
        Ok(0)
    }
    fn wait_deadline(&mut self, min_events: usize, _timeout_ns: Option<u64>) -> Result<usize, Self::Error> { self.wait(min_events) }

}

fn main() {
    const ITERS: u64 = 2_000_000;
    let mode = std::env::args().nth(1).unwrap_or_else(|| "idle".into());
    let emit_event = mode == "ready";
    let mut runtime =
        build_balanced_runtime::<ProbeDriver, EmptyTask>(ProbeDriver { emit_event }, 4);
    let mut total = 0usize;
    for now in 0..ITERS {
        let tick = runtime
            .tick_completions_or_wait(now, 1, 1, 1)
            .expect("tick");
        total += tick.completions.len();
    }
    println!("{mode}: {total}");
    black_box(total);
}
