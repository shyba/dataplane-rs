use std::net::UdpSocket;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicUsize, Ordering};

use dataplane::task::{NativeTask, NativeTaskCx, StepResult};
use dataplane::{net, ProfileKind, Runtime, RuntimeError};

static STEPS: AtomicUsize = AtomicUsize::new(0);

struct ParkOnceThenComplete;

impl NativeTask for ParkOnceThenComplete {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        if STEPS.fetch_add(1, Ordering::Relaxed) == 0 {
            StepResult::Parked
        } else {
            StepResult::Complete
        }
    }
}

struct ParkForever;

impl NativeTask for ParkForever {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        StepResult::Parked
    }
}

#[test]
fn run_reports_stall_instead_of_livelocking_on_unwakeable_parked_tasks() {
    let mut runtime = Runtime::<ParkForever>::builder()
        .profile(ProfileKind::Balanced)
        .build()
        .expect("build");
    runtime.spawn(ParkForever).expect("spawn");

    match runtime.run() {
        Err(RuntimeError::Stalled { parked_tasks }) => assert_eq!(parked_tasks, 1),
        other => panic!("expected Stalled, got {other:?}"),
    }
}

#[test]
fn submitted_io_completion_wakes_parked_task() {
    STEPS.store(0, Ordering::Relaxed);
    let mut runtime = Runtime::<ParkOnceThenComplete>::builder()
        .profile(ProfileKind::Balanced)
        .build()
        .expect("build");

    let receiver = UdpSocket::bind("127.0.0.1:0").expect("bind");
    let sender = UdpSocket::bind("127.0.0.1:0").expect("bind sender");
    let mut buf = vec![0u8; 64];

    let task = runtime.spawn(ParkOnceThenComplete).expect("spawn");
    // First tick parks the task.
    runtime.tick(16, 16).expect("tick");
    assert_eq!(STEPS.load(Ordering::Relaxed), 1);

    runtime
        .submit(
            net::NetOp::UdpRecv {
                fd: receiver.as_raw_fd(),
                ptr: buf.as_mut_ptr(),
                len: buf.len(),
            },
            Some(task),
        )
        .expect("submit");
    sender
        .send_to(b"wake", receiver.local_addr().expect("addr"))
        .expect("send");

    runtime.run().expect("run should complete after wake");
    assert_eq!(STEPS.load(Ordering::Relaxed), 2);
    assert!(!runtime.has_work());
}
