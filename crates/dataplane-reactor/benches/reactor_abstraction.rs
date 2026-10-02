#[cfg(target_os = "linux")]
use std::hint::black_box;
#[cfg(target_os = "linux")]
use std::os::fd::RawFd;

#[cfg(target_os = "linux")]
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
#[cfg(target_os = "linux")]
use dataplane_reactor::reactor::adaptive::{ReactorBackend, UnifiedReactor};
#[cfg(target_os = "linux")]
use dataplane_reactor::reactor::{NetEvent, NetOp, OpToken, Reactor};

#[cfg(target_os = "linux")]
fn bench_reactor_abstraction_empty(c: &mut Criterion) {
    let mut group = c.benchmark_group("reactor/abstraction_empty");
    group.throughput(Throughput::Elements(1));

    group.bench_function("unified_syscall", |b| {
        let mut reactor = UnifiedReactor::new(64, ReactorBackend::Syscall)
            .expect("create unified syscall reactor");
        b.iter(|| {
            let submitted = reactor.submit_pending().expect("submit pending");
            black_box(submitted);
            let events = reactor.poll(false).expect("poll");
            black_box(events.len());
        });
    });

    if UnifiedReactor::new(64, ReactorBackend::IoUring).is_ok() {
        group.bench_function("unified_uring", |b| {
            let mut reactor =
                UnifiedReactor::new(64, ReactorBackend::IoUring).expect("create unified uring");
            b.iter(|| {
                let submitted = reactor.submit_pending().expect("submit pending");
                black_box(submitted);
                let events = reactor.poll(false).expect("poll");
                black_box(events.len());
            });
        });
    } else {
        eprintln!("reactor_abstraction: io_uring unavailable, skipping uring empty benchmarks");
    }

    group.finish();
}

#[cfg(target_os = "linux")]
fn bench_reactor_abstraction_submit_send0(c: &mut Criterion) {
    let mut group = c.benchmark_group("reactor/abstraction_submit_send0");
    group.throughput(Throughput::Elements(1));
    let (tx_fd, rx_fd) = socket_pair_stream_nonblocking();
    let zero = [0u8; 1];

    group.bench_function(BenchmarkId::new("unified_syscall", "send0"), |b| {
        let mut reactor = UnifiedReactor::new(64, ReactorBackend::Syscall)
            .expect("create unified syscall reactor");
        b.iter(|| {
            // SAFETY: zero-length send uses a live socket and stable stack buffer.
            let token = unsafe { reactor.submit(NetOp::Send { fd: tx_fd, ptr: zero.as_ptr(), len: 0 }) }
                .expect("submit send");
            black_box(token);
            black_box(reactor.submit_pending().expect("submit pending"));
            let result = wait_token(&mut reactor, token);
            black_box(result);
        });
    });

    if UnifiedReactor::new(64, ReactorBackend::IoUring).is_ok() {
        group.bench_function(BenchmarkId::new("unified_uring", "send0"), |b| {
            let mut reactor =
                UnifiedReactor::new(64, ReactorBackend::IoUring).expect("create unified uring");
            b.iter(|| {
                // SAFETY: zero-length send uses a live socket and stable stack buffer.
                let token = unsafe { reactor.submit(NetOp::Send { fd: tx_fd, ptr: zero.as_ptr(), len: 0 }) }
                    .expect("submit send");
                black_box(token);
                black_box(reactor.submit_pending().expect("submit pending"));
                let result = wait_token(&mut reactor, token);
                black_box(result);
            });
        });
    } else {
        eprintln!("reactor_abstraction: io_uring unavailable, skipping uring send0 benchmarks");
    }

    close_fd(tx_fd);
    close_fd(rx_fd);
    group.finish();
}

#[cfg(target_os = "linux")]
fn wait_token<R: Reactor<Error = std::io::Error>>(reactor: &mut R, token: OpToken) -> i32 {
    loop {
        let events = reactor.poll(false).expect("poll");
        for event in events {
            let NetEvent::OpComplete {
                token: done,
                result,
                ..
            } = event;
            if done == token {
                return result;
            }
        }
        std::hint::spin_loop();
    }
}

#[cfg(target_os = "linux")]
fn socket_pair_stream_nonblocking() -> (RawFd, RawFd) {
    let mut fds = [0i32; 2];
    let rc = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
    assert_eq!(rc, 0, "socketpair failed");
    set_nonblocking_fd(fds[0]);
    set_nonblocking_fd(fds[1]);
    (fds[0], fds[1])
}

#[cfg(target_os = "linux")]
fn set_nonblocking_fd(fd: RawFd) {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    assert!(flags >= 0, "fcntl(F_GETFL) failed");
    let rc = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    assert_eq!(rc, 0, "fcntl(F_SETFL) failed");
}

#[cfg(target_os = "linux")]
fn close_fd(fd: RawFd) {
    let _ = unsafe { libc::close(fd) };
}

#[cfg(target_os = "linux")]
criterion_group!(
    reactor_abstraction_benches,
    bench_reactor_abstraction_empty,
    bench_reactor_abstraction_submit_send0
);
#[cfg(target_os = "linux")]
criterion_main!(reactor_abstraction_benches);

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("reactor_abstraction bench is linux-only");
}
