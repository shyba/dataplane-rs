//! Minimal dataplane consumer: spawn a few counter tasks and drive the
//! runtime to completion with the default (Auto) reactor backend.

use dataplane::task::{NativeTask, NativeTaskCx, StepResult};
use dataplane::{ProfileKind, Runtime};

struct Countdown {
    label: &'static str,
    steps_left: u32,
}

impl NativeTask for Countdown {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        if self.steps_left == 0 {
            println!("{} done", self.label);
            return StepResult::Complete;
        }
        self.steps_left -= 1;
        StepResult::Ready
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = Runtime::<Countdown>::builder()
        .profile(ProfileKind::Balanced)
        .task_capacity(8)
        .build()?;

    runtime
        .spawn(Countdown {
            label: "alpha",
            steps_left: 3,
        })
        .expect("task capacity");
    runtime
        .spawn(Countdown {
            label: "beta",
            steps_left: 5,
        })
        .expect("task capacity");

    runtime.run()?;
    println!("runtime idle, profile = {:?}", runtime.profile_kind());
    Ok(())
}
