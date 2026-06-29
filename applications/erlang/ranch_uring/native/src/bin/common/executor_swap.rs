use std::cell::{Cell, RefCell};
use std::thread;

use io_uring::{opcode, squeue, types, IoUring};
use ranch_uring_nif::local_exec::LocalExec;

#[derive(Clone, Copy, Debug)]
pub enum BenchCase {
    Serial,
    Parallel,
    Mixed,
}

#[derive(Clone, Copy, Debug)]
pub enum JobKind {
    Cheap,
    Medium,
    Heavy,
}

#[derive(Clone, Copy, Debug)]
pub struct WorkItem {
    pub slot: usize,
    pub kind: JobKind,
}

#[derive(Clone, Debug)]
pub struct Workload {
    pub shard_events: Vec<Vec<WorkItem>>,
    pub slot_counts: Vec<usize>,
    pub total_ops: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct BenchConfig {
    pub shards: usize,
    pub total_ops: usize,
    pub sessions: usize,
    pub hot_sessions: usize,
    pub hot_ratio: u32,
    pub cheap_ratio: u32,
    pub medium_ratio: u32,
    pub heavy_ratio: u32,
    pub session_budget: usize,
    pub runnable_budget: usize,
    pub drain_session: bool,
    pub case: BenchCase,
    pub seed: u64,
    pub medium_iters: usize,
    pub heavy_iters: usize,
    pub io_mode: IoMode,
    pub uring_link_mode: UringLinkMode,
    pub uring_link_batch: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum IoMode {
    GetTid,
    DevNullWrite,
    DevNullRead,
    UringNop,
    UringDevNullWrite,
    UringDevNullRead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UringLinkMode {
    Off,
    Soft,
    Hard,
}

impl BenchConfig {
    pub fn from_env() -> Self {
        let shards = parse_env_usize("EXEC_SWAP_SHARDS", default_shards()).max(1);
        let sessions = parse_env_usize("EXEC_SWAP_SESSIONS", 4096).max(1);
        let hot_sessions =
            parse_env_usize("EXEC_SWAP_HOT_SESSIONS", (sessions / 8).max(1)).clamp(1, sessions);
        let case = match std::env::var("EXEC_SWAP_CASE").ok().as_deref() {
            Some("serial") => BenchCase::Serial,
            Some("parallel") => BenchCase::Parallel,
            _ => BenchCase::Mixed,
        };
        Self {
            shards,
            total_ops: parse_env_usize("EXEC_SWAP_TOTAL_OPS", 2_000_000).max(1),
            sessions,
            hot_sessions,
            hot_ratio: parse_env_u32("EXEC_SWAP_HOT_RATIO", 80).min(100),
            cheap_ratio: parse_env_u32("EXEC_SWAP_CHEAP_RATIO", 70),
            medium_ratio: parse_env_u32("EXEC_SWAP_MEDIUM_RATIO", 20),
            heavy_ratio: parse_env_u32("EXEC_SWAP_HEAVY_RATIO", 10),
            session_budget: parse_env_usize("EXEC_SWAP_SESSION_BUDGET", 64).max(1),
            runnable_budget: parse_env_usize("EXEC_SWAP_RUNNABLE_BUDGET", 256).max(1),
            drain_session: std::env::var("EXEC_SWAP_DRAIN_SESSION").ok().as_deref() == Some("1"),
            case,
            seed: parse_env_u64("EXEC_SWAP_SEED", 0x9E37_79B9_7F4A_7C15),
            medium_iters: parse_env_usize("EXEC_SWAP_MEDIUM_ITERS", 16),
            heavy_iters: parse_env_usize("EXEC_SWAP_HEAVY_ITERS", 64),
            io_mode: parse_io_mode(),
            uring_link_mode: parse_uring_link_mode(),
            uring_link_batch: parse_env_usize("EXEC_SWAP_URING_LINK_BATCH", 1).clamp(1, 64),
        }
    }
}

pub fn generate_workload(cfg: &BenchConfig) -> Workload {
    let mut rng = XorShift64::new(cfg.seed);
    let mut shard_events: Vec<Vec<WorkItem>> = (0..cfg.shards).map(|_| Vec::new()).collect();
    let mut slot_counts = vec![0usize; cfg.shards];
    let cold_sessions = cfg.sessions.saturating_sub(cfg.hot_sessions);

    let kind_total = (cfg.cheap_ratio + cfg.medium_ratio + cfg.heavy_ratio).max(1);
    for _ in 0..cfg.total_ops {
        let session = match cfg.case {
            BenchCase::Serial => rng.next_usize(cfg.hot_sessions),
            BenchCase::Parallel => rng.next_usize(cfg.sessions),
            BenchCase::Mixed => {
                let choose_hot = rng.pct() < cfg.hot_ratio || cold_sessions == 0;
                if choose_hot {
                    rng.next_usize(cfg.hot_sessions)
                } else {
                    cfg.hot_sessions + rng.next_usize(cold_sessions)
                }
            }
        };
        let kind_pick = (rng.next_u64() % kind_total as u64) as u32;
        let kind = if kind_pick < cfg.cheap_ratio {
            JobKind::Cheap
        } else if kind_pick < cfg.cheap_ratio + cfg.medium_ratio {
            JobKind::Medium
        } else {
            JobKind::Heavy
        };
        let shard = session % cfg.shards;
        let slot = session / cfg.shards;
        slot_counts[shard] = slot_counts[shard].max(slot + 1);
        shard_events[shard].push(WorkItem { slot, kind });
    }
    Workload {
        shard_events,
        slot_counts,
        total_ops: cfg.total_ops,
    }
}

pub fn run_ours(cfg: &BenchConfig, workload: &Workload) -> usize {
    let mut joins = Vec::with_capacity(cfg.shards);
    for shard in 0..cfg.shards {
        let shard_events = workload.shard_events[shard].clone();
        let slot_count = workload.slot_counts[shard];
        let runnable_budget = cfg.runnable_budget;
        let session_budget = cfg.session_budget;
        let drain_session = cfg.drain_session;
        let medium_iters = cfg.medium_iters;
        let heavy_iters = cfg.heavy_iters;
        let io_mode = cfg.io_mode;
        joins.push(
            thread::Builder::new()
                .name(format!("exec-swap-ours-{shard}"))
                .spawn(move || {
                    pin_current_thread(shard);
                    if slot_count == 0 {
                        return 0usize;
                    }
                    let mut exec = LocalExec::new(slot_count);
                    for item in shard_events {
                        exec.push(item.slot, item.kind);
                    }
                    let mut completed = 0usize;
                    while exec.has_work() {
                        completed +=
                            exec.drain(runnable_budget, session_budget, drain_session, |job| {
                                execute_job(job, io_mode, medium_iters, heavy_iters)
                            });
                    }
                    flush_io_mode(io_mode);
                    completed
                })
                .expect("spawn ours shard"),
        );
    }

    joins
        .into_iter()
        .map(|join| join.join().expect("join ours shard"))
        .sum()
}

#[inline(always)]
pub fn execute_job(job: JobKind, io_mode: IoMode, medium_iters: usize, heavy_iters: usize) {
    run_io_mode(io_mode);
    apply_job_weight(job, medium_iters, heavy_iters);
}

pub fn run_io_mode(io_mode: IoMode) {
    run_base_syscall(io_mode);
}

pub fn flush_io_mode(io_mode: IoMode) {
    match io_mode {
        IoMode::UringNop | IoMode::UringDevNullWrite | IoMode::UringDevNullRead => {
            flush_uring_state();
        }
        _ => {}
    }
}

pub fn apply_job_weight(job: JobKind, medium_iters: usize, heavy_iters: usize) {
    match job {
        JobKind::Cheap => {}
        JobKind::Medium => burn_cycles(medium_iters),
        JobKind::Heavy => burn_cycles(heavy_iters),
    }
}

#[inline(always)]
fn run_base_syscall(mode: IoMode) {
    match mode {
        IoMode::GetTid => {
            let tid = unsafe { libc::syscall(libc::SYS_gettid) };
            std::hint::black_box(tid);
        }
        IoMode::DevNullWrite => {
            let fd = devnull_write_fd();
            let data = [0u8; 64];
            let rc = unsafe { libc::write(fd, data.as_ptr() as *const libc::c_void, data.len()) };
            std::hint::black_box(rc);
        }
        IoMode::DevNullRead => {
            let fd = devnull_read_fd();
            let mut buf = [0u8; 64];
            let rc = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            std::hint::black_box(rc);
        }
        IoMode::UringNop => run_uring_nop(),
        IoMode::UringDevNullWrite => run_uring_devnull_write(),
        IoMode::UringDevNullRead => run_uring_devnull_read(),
    }
}

#[inline(always)]
fn burn_cycles(iters: usize) {
    let mut acc = 0u64;
    let mut i = 0usize;
    while i < iters {
        acc = acc
            .wrapping_mul(6364136223846793005)
            .wrapping_add(i as u64 ^ 0x9E37_79B9);
        i += 1;
    }
    std::hint::black_box(acc);
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn parse_env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(default)
}

fn parse_env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(default)
}

fn parse_io_mode() -> IoMode {
    match std::env::var("EXEC_SWAP_IO_MODE").ok().as_deref() {
        Some("devnull_write") => IoMode::DevNullWrite,
        Some("devnull_read") => IoMode::DevNullRead,
        Some("uring_nop") => IoMode::UringNop,
        Some("uring_devnull_write") | Some("uring_write") => IoMode::UringDevNullWrite,
        Some("uring_devnull_read") | Some("uring_read") => IoMode::UringDevNullRead,
        _ => IoMode::GetTid,
    }
}

fn parse_uring_link_mode() -> UringLinkMode {
    match std::env::var("EXEC_SWAP_URING_LINK")
        .ok()
        .as_deref()
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("soft") | Some("link") | Some("1") => UringLinkMode::Soft,
        Some("hard") | Some("hardlink") => UringLinkMode::Hard,
        _ => UringLinkMode::Off,
    }
}

fn default_shards() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .max(1)
}

#[inline]
pub fn pin_current_thread(shard: usize) {
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

struct XorShift64 {
    state: u64,
}

thread_local! {
    static DEVNULL_READ_FD: Cell<i32> = const { Cell::new(-1) };
    static DEVNULL_WRITE_FD: Cell<i32> = const { Cell::new(-1) };
    static URING_STATE: RefCell<Option<UringState>> = const { RefCell::new(None) };
}

fn devnull_read_fd() -> i32 {
    DEVNULL_READ_FD.with(|cell| {
        let current = cell.get();
        if current >= 0 {
            return current;
        }
        let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
        assert!(fd >= 0, "failed to open /dev/null for read");
        cell.set(fd);
        fd
    })
}

fn devnull_write_fd() -> i32 {
    DEVNULL_WRITE_FD.with(|cell| {
        let current = cell.get();
        if current >= 0 {
            return current;
        }
        let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC) };
        assert!(fd >= 0, "failed to open /dev/null for write");
        cell.set(fd);
        fd
    })
}

struct UringState {
    ring: IoUring,
    read_fd: i32,
    write_fd: i32,
    write_buf: [u8; 64],
    read_buf: [u8; 64],
    pending: usize,
    link_mode: UringLinkMode,
    link_batch: usize,
}

fn run_uring_nop() {
    with_uring_state(|state| {
        let entry = opcode::Nop::new().build().user_data(1);
        let rc = submit_single_entry(state, entry);
        std::hint::black_box(rc);
    });
}

fn run_uring_devnull_write() {
    with_uring_state(|state| {
        if state.write_fd < 0 {
            let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC) };
            assert!(fd >= 0, "failed to open /dev/null for uring write");
            state.write_fd = fd;
            state.write_buf.fill(0);
        }
        let entry = opcode::Write::new(
            types::Fd(state.write_fd),
            state.write_buf.as_ptr(),
            state.write_buf.len() as u32,
        )
        .build()
        .user_data(1);
        let rc = submit_single_entry(state, entry);
        std::hint::black_box(rc);
    });
}

fn run_uring_devnull_read() {
    with_uring_state(|state| {
        if state.read_fd < 0 {
            let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
            assert!(fd >= 0, "failed to open /dev/null for uring read");
            state.read_fd = fd;
        }
        let entry = opcode::Read::new(
            types::Fd(state.read_fd),
            state.read_buf.as_mut_ptr(),
            state.read_buf.len() as u32,
        )
        .build()
        .user_data(1);
        let rc = submit_single_entry(state, entry);
        std::hint::black_box(rc);
    });
}

fn with_uring_state<R>(f: impl FnOnce(&mut UringState) -> R) -> R {
    URING_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.is_none() {
            let link_mode = parse_uring_link_mode();
            let link_batch = parse_env_usize("EXEC_SWAP_URING_LINK_BATCH", 1).clamp(1, 64);
            let ring = IoUring::new(64).expect("create io_uring");
            *state = Some(UringState {
                ring,
                read_fd: -1,
                write_fd: -1,
                write_buf: [0u8; 64],
                read_buf: [0u8; 64],
                pending: 0,
                link_mode,
                link_batch,
            });
        }
        let inner = state.as_mut().expect("uring state initialized");
        f(inner)
    })
}

#[inline(always)]
fn submit_single_entry(state: &mut UringState, entry: squeue::Entry) -> i32 {
    if state.ring.submission().is_full() {
        let _ = reap_uring_pending(state);
    }

    let link_enabled = state.link_mode != UringLinkMode::Off && state.link_batch > 1;
    let entry = if link_enabled && (state.pending + 1) < state.link_batch {
        match state.link_mode {
            UringLinkMode::Soft => entry.flags(squeue::Flags::IO_LINK),
            UringLinkMode::Hard => entry.flags(squeue::Flags::IO_HARDLINK),
            UringLinkMode::Off => entry,
        }
    } else {
        entry
    };

    unsafe {
        state
            .ring
            .submission()
            .push(&entry)
            .expect("push uring bench entry");
    }
    state.pending += 1;

    if link_enabled {
        if state.pending >= state.link_batch {
            reap_uring_pending(state)
        } else {
            0
        }
    } else {
        reap_uring_pending(state)
    }
}

#[inline(always)]
fn reap_uring_pending(state: &mut UringState) -> i32 {
    let mut remaining = state.pending;
    if remaining == 0 {
        return 0;
    }

    let mut last_rc = 0i32;
    while remaining > 0 {
        state
            .ring
            .submit_and_wait(remaining)
            .expect("submit_and_wait uring bench");
        let mut cq = state.ring.completion();
        while let Some(cqe) = cq.next() {
            last_rc = cqe.result();
            remaining -= 1;
            if remaining == 0 {
                break;
            }
        }
    }
    state.pending = 0;
    last_rc
}

fn flush_uring_state() {
    URING_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if let Some(inner) = state.as_mut() {
            let _ = reap_uring_pending(inner);
        }
    });
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0xDEAD_BEEF_CAFE_BABE
            } else {
                seed
            },
        }
    }

    #[inline(always)]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    #[inline(always)]
    fn next_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }

    #[inline(always)]
    fn pct(&mut self) -> u32 {
        (self.next_u64() % 100) as u32
    }
}
