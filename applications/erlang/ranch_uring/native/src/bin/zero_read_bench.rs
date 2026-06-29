use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::thread;
use std::time::Instant;

use io_uring::{opcode, types, IoUring};
#[path = "common/step_stats.rs"]
mod step_stats;
use step_stats::StepStats;

const DEFAULT_ITERS: usize = 5_000_000;
const DEFAULT_BYTES: usize = 512;
const DEFAULT_RING_ENTRIES: u32 = 256;
const DEFAULT_QD: usize = 1;
const PAGE_ALIGN: usize = 4096;
const DEFAULT_SQPOLL_IDLE_MS: u32 = 2000;
const REGISTERED_ALIGN_BYTES: usize = 64;

#[derive(Clone, Copy, Debug)]
enum Mode {
    Direct,
    Uring,
}

#[derive(Clone, Copy, Debug)]
enum SqpollMode {
    Off,
    Try,
    Require,
}

fn main() {
    let mode = match std::env::var("ZERO_READ_MODE").ok().as_deref() {
        Some("uring") => Mode::Uring,
        _ => Mode::Direct,
    };
    let iters = parse_env_usize("ZERO_READ_ITERS", DEFAULT_ITERS);
    let bytes = parse_env_usize("ZERO_READ_BYTES", DEFAULT_BYTES);
    let qd = parse_env_usize("ZERO_READ_QD", DEFAULT_QD);
    let align_4096 = parse_env_bool("ZERO_READ_ALIGN_4096", false);
    let ring_entries =
        parse_env_usize("ZERO_READ_URING_ENTRIES", DEFAULT_RING_ENTRIES as usize) as u32;
    let sqpoll_mode = parse_sqpoll_mode();
    let sqpoll_idle_ms =
        parse_env_usize("ZERO_READ_SQPOLL_IDLE_MS", DEFAULT_SQPOLL_IDLE_MS as usize) as u32;
    let sqpoll_cpu = std::env::var("ZERO_READ_SQPOLL_CPU")
        .ok()
        .and_then(|value| value.parse::<u32>().ok());

    pin_current_thread(0);

    println!(
        "zero_read_bench mode={:?} iters={} bytes={} qd={} align_4096={} ring_entries={} sqpoll_mode={:?} sqpoll_cpu={:?}",
        mode, iters, bytes, qd, align_4096, ring_entries, sqpoll_mode, sqpoll_cpu
    );

    let mut step_stats = StepStats::from_env("ZERO_READ_STEP_STATS");
    let started = Instant::now();
    match mode {
        Mode::Direct => run_direct(iters, bytes, align_4096, &mut step_stats),
        Mode::Uring => run_uring(
            iters,
            bytes,
            qd,
            align_4096,
            ring_entries,
            sqpoll_mode,
            sqpoll_idle_ms,
            sqpoll_cpu,
            &mut step_stats,
        ),
    }
    let elapsed = started.elapsed();
    let ops_per_sec = iters as f64 / elapsed.as_secs_f64();
    let bytes_per_sec = (iters * bytes) as f64 / elapsed.as_secs_f64();
    println!(
        "iters={} elapsed_ms={:.2} ops_per_sec={:.0} bytes_per_sec={:.0}",
        iters,
        elapsed.as_secs_f64() * 1000.0,
        ops_per_sec,
        bytes_per_sec
    );
    step_stats.print("zero_read_bench");
}

fn run_direct(iters: usize, bytes: usize, align_4096: bool, step_stats: &mut StepStats) {
    let mut file = File::open("/dev/zero").expect("open /dev/zero");
    let mut buf = if align_4096 {
        WorkBuf::new(bytes, true)
    } else {
        WorkBuf::aligned(
            bytes,
            bytes.max(REGISTERED_ALIGN_BYTES),
            REGISTERED_ALIGN_BYTES,
        )
    };
    let mut chunk_left = 1024usize;
    let mut chunk_start = step_stats.begin();
    for _ in 0..iters {
        file.read_exact(buf.as_mut_slice()).expect("direct read");
        chunk_left -= 1;
        if chunk_left == 0 {
            step_stats.end("direct_read_chunk", chunk_start);
            chunk_start = step_stats.begin();
            chunk_left = 1024;
        }
    }
    if chunk_left != 1024 {
        step_stats.end("direct_read_chunk", chunk_start);
    }
    std::hint::black_box(buf.as_slice());
}

#[derive(Clone, Copy, Default)]
struct ReadSlot {
    filled: usize,
    in_flight: bool,
}

#[allow(clippy::too_many_arguments)]
fn run_uring(
    iters: usize,
    bytes: usize,
    qd: usize,
    align_4096: bool,
    ring_entries: u32,
    sqpoll_mode: SqpollMode,
    sqpoll_idle_ms: u32,
    sqpoll_cpu: Option<u32>,
    step_stats: &mut StepStats,
) {
    let file = File::open("/dev/zero").expect("open /dev/zero");
    let fd = file.as_raw_fd();
    let mut ring = build_ring(ring_entries, sqpoll_mode, sqpoll_idle_ms, sqpoll_cpu);
    println!("sqpoll_accepted={}", ring.params().is_setup_sqpoll());
    assert!(bytes <= u32::MAX as usize, "bytes must fit in u32");
    assert!(qd > 0, "qd must be > 0");
    assert!(
        qd <= ring_entries as usize,
        "qd {} must be <= ring_entries {}",
        qd,
        ring_entries
    );

    let mut bufs: Vec<WorkBuf> = if align_4096 {
        (0..qd).map(|_| WorkBuf::new(bytes, true)).collect()
    } else {
        let slot_bytes = bytes.max(REGISTERED_ALIGN_BYTES);
        let arena_bytes = slot_bytes
            .checked_mul(qd)
            .expect("registered arena size overflow");
        let mut arena = WorkBuf::aligned(arena_bytes, arena_bytes, REGISTERED_ALIGN_BYTES);
        let base = arena.as_mut_slice().as_mut_ptr();
        let mut iovecs = Vec::with_capacity(qd);
        for slot in 0..qd {
            let ptr = unsafe { base.add(slot * slot_bytes) };
            iovecs.push(libc::iovec {
                iov_base: ptr.cast(),
                iov_len: slot_bytes,
            });
        }
        unsafe {
            ring.submitter()
                .register_buffers(&iovecs)
                .expect("register buffers");
        }
        let mut bufs = Vec::with_capacity(qd);
        for slot in 0..qd {
            let start = slot * slot_bytes;
            bufs.push(WorkBuf::from_registered_slot(
                arena.as_slice(),
                start,
                bytes,
            ));
        }
        std::mem::forget(arena);
        bufs
    };
    let iovecs: Vec<libc::iovec> = if align_4096 {
        bufs.iter_mut()
            .map(|buf| libc::iovec {
                iov_base: buf.as_mut_slice().as_mut_ptr().cast(),
                iov_len: buf.as_slice().len(),
            })
            .collect()
    } else {
        Vec::new()
    };
    if align_4096 {
        unsafe {
            ring.submitter()
                .register_buffers(&iovecs)
                .expect("register buffers");
        }
    }
    let mut slots = vec![ReadSlot::default(); qd];
    let mut free_slots: Vec<usize> = (0..qd).rev().collect();
    let mut resubmit: Vec<(usize, usize)> = Vec::with_capacity(qd);

    let mut submitted = 0usize;
    let mut completed = 0usize;
    let mut in_flight = 0usize;

    while completed < iters {
        while in_flight < qd && submitted < iters {
            let slot = free_slots.pop().expect("free slot available");
            slots[slot].filled = 0;
            slots[slot].in_flight = true;
            queue_read_for_slot(
                &mut ring,
                fd,
                bufs[slot].as_mut_slice(),
                0,
                slot as u16,
                slot as u64,
                step_stats,
            );
            submitted += 1;
            in_flight += 1;
        }

        if in_flight == 0 {
            break;
        }

        let step = step_stats.begin();
        ring.submit_and_wait(1).expect("submit_and_wait");
        step_stats.end("wait_submit", step);
        resubmit.clear();
        {
            let step = step_stats.begin();
            let cq = ring.completion();
            for cqe in cq {
                let slot = cqe.user_data() as usize;
                assert!(slot < qd, "invalid slot {}", slot);
                let res = cqe.result();
                assert!(res > 0, "uring read returned {res}");
                let got = res as usize;

                let state = &mut slots[slot];
                assert!(state.in_flight, "completion on inactive slot {}", slot);
                state.filled += got;
                if state.filled < bytes {
                    resubmit.push((slot, state.filled));
                } else {
                    state.in_flight = false;
                    state.filled = 0;
                    free_slots.push(slot);
                    completed += 1;
                    in_flight -= 1;
                }
                std::hint::black_box(cqe.user_data());
            }
            step_stats.end("cq_drain", step);
        }
        for (slot, filled) in resubmit.drain(..) {
            queue_read_for_slot(
                &mut ring,
                fd,
                bufs[slot].as_mut_slice(),
                filled,
                slot as u16,
                slot as u64,
                step_stats,
            );
        }
    }

    assert_eq!(
        completed, iters,
        "completed {} != iters {}",
        completed, iters
    );
    for buf in &bufs {
        std::hint::black_box(buf.as_slice());
    }
}

fn build_ring(
    ring_entries: u32,
    sqpoll_mode: SqpollMode,
    sqpoll_idle_ms: u32,
    sqpoll_cpu: Option<u32>,
) -> IoUring {
    if matches!(sqpoll_mode, SqpollMode::Off) {
        return IoUring::new(ring_entries).expect("create io_uring");
    }

    let mut builder = IoUring::builder();
    builder.setup_clamp();
    builder.setup_sqpoll(sqpoll_idle_ms);
    if let Some(cpu) = sqpoll_cpu {
        builder.setup_sqpoll_cpu(cpu);
    }

    match builder.build(ring_entries) {
        Ok(ring) => ring,
        Err(err) if matches!(sqpoll_mode, SqpollMode::Try) => {
            let errno = err.raw_os_error().unwrap_or(0);
            if matches!(
                errno,
                libc::EPERM | libc::EINVAL | libc::EOPNOTSUPP | libc::ENOSYS
            ) {
                IoUring::new(ring_entries).expect("create io_uring fallback")
            } else {
                panic!("create sqpoll io_uring failed with errno={errno}");
            }
        }
        Err(err) => panic!("create sqpoll io_uring failed: {err}"),
    }
}

fn queue_read_for_slot(
    ring: &mut IoUring,
    fd: i32,
    buf: &mut [u8],
    offset: usize,
    buf_index: u16,
    user_data: u64,
    step_stats: &mut StepStats,
) {
    let remaining = buf.len().saturating_sub(offset);
    assert!(remaining > 0, "remaining read length must be > 0");
    let ptr = unsafe { buf.as_mut_ptr().add(offset) };
    let entry = opcode::ReadFixed::new(types::Fd(fd), ptr, remaining as u32, buf_index)
        .offset(u64::MAX)
        .build()
        .user_data(user_data);
    let step = step_stats.begin();
    unsafe {
        if ring.submission().push(&entry).is_err() {
            let submit_step = step_stats.begin();
            ring.submit().expect("submit io_uring queue");
            step_stats.end("submit_sq_space", submit_step);
            ring.submission()
                .push(&entry)
                .expect("queue io_uring read after submit");
        }
    }
    step_stats.end("sq_fill", step);
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn parse_env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name).ok().as_deref() {
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("on") => true,
        Some("0") | Some("false") | Some("FALSE") | Some("no") | Some("off") => false,
        _ => default,
    }
}

fn parse_sqpoll_mode() -> SqpollMode {
    match std::env::var("ZERO_READ_SQPOLL")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("try") => SqpollMode::Try,
        Some("require") | Some("on") | Some("1") => SqpollMode::Require,
        _ => SqpollMode::Off,
    }
}

enum WorkBuf {
    Plain(Vec<u8>),
    Aligned(AlignedBuf),
    RegisteredSlot { base: *mut u8, len: usize },
}

impl WorkBuf {
    fn new(len: usize, align_4096: bool) -> Self {
        if align_4096 {
            Self::Aligned(AlignedBuf::new(len, PAGE_ALIGN))
        } else {
            Self::Plain(vec![0u8; len])
        }
    }

    fn aligned(len: usize, page_bytes: usize, align: usize) -> Self {
        let reserve = len.max(page_bytes);
        Self::Aligned(AlignedBuf::new(reserve, align))
    }

    fn from_registered_slot(page: &[u8], offset: usize, len: usize) -> Self {
        assert!(offset + len <= page.len(), "registered slot out of bounds");
        let base = unsafe { page.as_ptr().add(offset) as *mut u8 };
        Self::RegisteredSlot { base, len }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            Self::Plain(buf) => buf.as_mut_slice(),
            Self::Aligned(buf) => buf.as_mut_slice(),
            Self::RegisteredSlot { base, len } => unsafe {
                std::slice::from_raw_parts_mut(*base, *len)
            },
        }
    }

    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Plain(buf) => buf.as_slice(),
            Self::Aligned(buf) => buf.as_slice(),
            Self::RegisteredSlot { base, len } => unsafe {
                std::slice::from_raw_parts(*base, *len)
            },
        }
    }
}

struct AlignedBuf {
    storage: Vec<u8>,
    offset: usize,
    len: usize,
}

impl AlignedBuf {
    fn new(len: usize, align: usize) -> Self {
        assert!(align.is_power_of_two(), "alignment must be power of two");
        let mut storage = vec![0u8; len + align - 1];
        let base = storage.as_mut_ptr() as usize;
        let aligned = (base + (align - 1)) & !(align - 1);
        let offset = aligned - base;
        Self {
            storage,
            offset,
            len,
        }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        let start = self.offset;
        &mut self.storage[start..start + self.len]
    }

    fn as_slice(&self) -> &[u8] {
        let start = self.offset;
        &self.storage[start..start + self.len]
    }
}

fn pin_current_thread(shard: usize) {
    let cpu_count = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    let cpu = shard % cpu_count;
    let mut set = unsafe { std::mem::zeroed::<libc::cpu_set_t>() };
    unsafe {
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(cpu, &mut set);
        let rc = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
        assert_eq!(rc, 0, "sched_setaffinity failed");
    }
}
