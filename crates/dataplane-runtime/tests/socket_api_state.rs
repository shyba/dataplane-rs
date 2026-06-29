#![cfg(feature = "erlang-nif")]

use dataplane_runtime::socket::{SessionState, SocketOpts};
use rustler::LocalPid;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

fn zero_pid() -> LocalPid {
    // rustler::LocalPid has no safe constructor outside a live NIF env.
    // This placeholder pid is only used by api_state unit tests.
    unsafe { std::mem::zeroed() }
}

fn make_session_state() -> SessionState {
    SessionState {
        session_id: 1,
        shard: 0,
        fd: -1,
        owner: Mutex::new(zero_pid()),
        opts: Mutex::new(SocketOpts::default()),
        mailbox_passive: AtomicBool::new(false),
        local: Mutex::new(None),
        peer: Mutex::new(None),
        api_state: AtomicUsize::new(0),
        closed: AtomicBool::new(false),
    }
}

#[test]
fn mark_api_closed_blocks_new_api_calls() {
    let session = make_session_state();
    assert!(session.begin_api_call());
    session.end_api_call();
    assert!(session.mark_api_closed());
    assert!(session.is_closed());
    assert!(!session.begin_api_call());
}

#[test]
fn mark_api_closed_waits_for_inflight_api_calls() {
    let session = Arc::new(make_session_state());
    let ready = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));

    let session_clone = Arc::clone(&session);
    let ready_clone = Arc::clone(&ready);
    let release_clone = Arc::clone(&release);
    let worker = thread::spawn(move || {
        assert!(session_clone.begin_api_call());
        ready_clone.wait();
        release_clone.wait();
        session_clone.end_api_call();
    });

    ready.wait();
    let session_clone = Arc::clone(&session);
    let closer = thread::spawn(move || session_clone.mark_api_closed());
    thread::yield_now();
    release.wait();
    assert!(closer.join().expect("closer should finish"));
    assert!(session.is_closed());
    worker.join().expect("worker should finish");
}
