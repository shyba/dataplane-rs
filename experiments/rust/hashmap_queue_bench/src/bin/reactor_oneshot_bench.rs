use dataplane_core_reactor::future_task::FutureTask;
use dataplane_core_reactor::mailbox_future::GlobalContext;
use dataplane_core_reactor::native_future_task::NativeFutureTask;
use dataplane_core_reactor::native_task::{
    NativeTask, NativeTaskCx, NativeTaskEngine, StepResult,
};
use oneshot::{channel, Receiver, Sender};
use std::cell::RefCell;
use std::env;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Barrier};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::task::Poll;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReactorKind {
    Boxed,
    Bump,
}

impl ReactorKind {
    fn from_env() -> Self {
        match env::var("REACTOR_KIND")
            .unwrap_or_else(|_| "bump".to_string())
            .as_str()
        {
            "boxed" => Self::Boxed,
            _ => Self::Bump,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Local,
    TwoShard,
    DirectTwoShard,
    DirectTwoShardNative,
    QueueTwoShard,
    QueueTwoShardFast,
}

impl Mode {
    fn from_env() -> Self {
        match env::var("ONESHOT_MODE")
            .unwrap_or_else(|_| "local".to_string())
            .as_str()
        {
            "two_shard" => Self::TwoShard,
            "direct_two_shard" => Self::DirectTwoShard,
            "direct_two_shard_native" => Self::DirectTwoShardNative,
            "queue_two_shard" => Self::QueueTwoShard,
            "queue_two_shard_fast" => Self::QueueTwoShardFast,
            _ => Self::Local,
        }
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn pin_to(core: usize) {
    if env::var("PIN_THREADS").ok().as_deref() == Some("0") {
        return;
    }
    if let Some(ids) = core_affinity::get_core_ids() {
        let idx = core.min(ids.len().saturating_sub(1));
        let _ = core_affinity::set_for_current(ids[idx]);
    }
}

#[inline(always)]
fn drive_future_to_completion<F, T>(future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let mut task = FutureTask::new_local(future);
    loop {
        match task.poll_dummy() {
            std::task::Poll::Ready(value) => return value,
            Poll::Pending => thread::yield_now(),
        }
    }
}

struct CountedNativeFutureTask<F> {
    inner: NativeFutureTask<F>,
    polls: Arc<AtomicUsize>,
}

impl<F> CountedNativeFutureTask<F>
where
    F: std::future::Future<Output = ()>,
{
    #[inline(always)]
    fn ready(future: F, polls: Arc<AtomicUsize>) -> Self {
        Self {
            inner: NativeFutureTask::ready(future),
            polls,
        }
    }
}

impl<F> NativeTask for CountedNativeFutureTask<F>
where
    F: std::future::Future<Output = ()>,
{
    #[inline(always)]
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        self.polls.fetch_add(1, Ordering::Relaxed);
        match self.inner.poll_dummy() {
            std::task::Poll::Ready(()) => StepResult::Complete,
            Poll::Pending => StepResult::Ready,
        }
    }
}

type BoxedUnitFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

#[derive(Clone)]
struct NativeCompatSpawner {
    inner: Rc<RefCell<NativeTaskEngine<NativeFutureTask<BoxedUnitFuture>>>>,
    pending_spawns: Rc<RefCell<Vec<BoxedUnitFuture>>>,
    running: Rc<std::cell::Cell<bool>>,
}

struct NativeCompatEngine {
    inner: Rc<RefCell<NativeTaskEngine<NativeFutureTask<BoxedUnitFuture>>>>,
    pending_spawns: Rc<RefCell<Vec<BoxedUnitFuture>>>,
    running: Rc<std::cell::Cell<bool>>,
}

type LocalAsyncEngine = NativeCompatEngine;
type LocalBumpAsyncEngine = NativeCompatEngine;

impl NativeCompatEngine {
    fn with_task_capacity(capacity: usize) -> Self {
        Self {
            inner: Rc::new(RefCell::new(NativeTaskEngine::with_task_capacity(capacity))),
            pending_spawns: Rc::new(RefCell::new(Vec::new())),
            running: Rc::new(std::cell::Cell::new(false)),
        }
    }

    fn spawner(&self) -> NativeCompatSpawner {
        NativeCompatSpawner {
            inner: self.inner.clone(),
            pending_spawns: self.pending_spawns.clone(),
            running: self.running.clone(),
        }
    }

    fn flush_pending_spawns(&self) {
        let mut pending = self.pending_spawns.borrow_mut();
        if pending.is_empty() {
            return;
        }
        let mut engine = self.inner.borrow_mut();
        for future in pending.drain(..) {
            engine.spawn(NativeFutureTask::ready(future));
        }
    }

    fn run_engine_hot(&self, hot: usize, budget: usize) -> usize {
        self.running.set(true);
        let progressed = self.inner.borrow_mut().run_hot_budget_dynamic(hot, budget);
        self.running.set(false);
        self.flush_pending_spawns();
        progressed
    }

    fn block_on<F, T>(&self, future: F) -> T
    where
        F: Future<Output = T> + 'static,
        T: 'static,
    {
        let out = Rc::new(RefCell::new(None::<T>));
        let out_task = out.clone();
        self.spawner().spawn_detached(async move {
            out_task.replace(Some(future.await));
        });

        loop {
            if let Some(value) = out.borrow_mut().take() {
                return value;
            }
            let progressed = self.run_engine_hot(0, 4096);
            if progressed == 0 {
                thread::yield_now();
            }
        }
    }

    fn run_until_stalled(&self) -> usize {
        let mut total = 0usize;
        loop {
            let progressed = self.run_engine_hot(0, 4096);
            total += progressed;
            if progressed == 0 {
                break;
            }
        }
        total
    }
}

impl NativeCompatSpawner {
    fn spawn_detached<F>(&self, future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        let boxed: BoxedUnitFuture = Box::pin(future);
        if self.running.get() {
            self.pending_spawns.borrow_mut().push(boxed);
            return;
        }
        self.inner.borrow_mut().spawn(NativeFutureTask::ready(boxed));
    }

    fn spawn_send_detached<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_detached(future);
    }
}

struct YieldOnce {
    yielded: bool,
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

fn boxed_yield_now() -> YieldOnce {
    YieldOnce { yielded: false }
}

fn bump_yield_now() -> YieldOnce {
    YieldOnce { yielded: false }
}

fn run_boxed_local(pairs: usize, burst: usize, sender_yields: usize) -> u64 {
    let capacity = env_usize("FLOODER_ENGINE_CAPACITY", pairs.saturating_mul(2).max(4096));
    let engine = LocalAsyncEngine::with_task_capacity(capacity);
    let spawner = engine.spawner();
    engine.block_on(async move {
        let completed = Arc::new(AtomicUsize::new(0));
        let checksum = Arc::new(AtomicU64::new(0));

        for i in 0..pairs {
            let (tx, rx) = channel::<u64>();

            let completed_rx = completed.clone();
            let checksum_rx = checksum.clone();
            spawner.spawn_send_detached(async move {
                let value = rx.await.expect("local boxed receiver");
                checksum_rx.fetch_add(value, Ordering::Relaxed);
                completed_rx.fetch_add(1, Ordering::Relaxed);
            });

            spawner.spawn_detached(async move {
                for _ in 0..sender_yields {
                    boxed_yield_now().await;
                }
                let _ = tx.send(i as u64);
            });

            if burst != 0 && ((i + 1) % burst) == 0 {
                boxed_yield_now().await;
            }
        }

        while completed.load(Ordering::Relaxed) != pairs {
            boxed_yield_now().await;
        }

        checksum.load(Ordering::Relaxed)
    })
}

fn run_bump_local(pairs: usize, burst: usize, sender_yields: usize) -> u64 {
    let capacity = env_usize("FLOODER_ENGINE_CAPACITY", pairs.saturating_mul(2).max(4096));
    let engine = LocalBumpAsyncEngine::with_task_capacity(capacity);
    let spawner = engine.spawner();
    engine.block_on(async move {
        let completed = Arc::new(AtomicUsize::new(0));
        let checksum = Arc::new(AtomicU64::new(0));

        for i in 0..pairs {
            let (tx, rx) = channel::<u64>();

            let completed_rx = completed.clone();
            let checksum_rx = checksum.clone();
            spawner.spawn_detached(async move {
                let value = rx.await.expect("local bump receiver");
                checksum_rx.fetch_add(value, Ordering::Relaxed);
                completed_rx.fetch_add(1, Ordering::Relaxed);
            });

            spawner.spawn_detached(async move {
                for _ in 0..sender_yields {
                    bump_yield_now().await;
                }
                let _ = tx.send(i as u64);
            });

            if burst != 0 && ((i + 1) % burst) == 0 {
                bump_yield_now().await;
            }
        }

        while completed.load(Ordering::Relaxed) != pairs {
            bump_yield_now().await;
        }

        checksum.load(Ordering::Relaxed)
    })
}

fn run_boxed_sender_shard(
    senders: Vec<(usize, Sender<u64>)>,
    burst: usize,
    sender_yields: usize,
    barrier: Arc<Barrier>,
    shard_id: usize,
) -> (u64, Duration) {
    pin_to(shard_id);
    let sender_count = senders.len();
    let capacity =
        env_usize("FLOODER_ENGINE_CAPACITY", sender_count.saturating_add(1024).max(4096));
    let engine = LocalAsyncEngine::with_task_capacity(capacity);
    let spawner = engine.spawner();
    barrier.wait();
    let start = Instant::now();
    let checksum = engine.block_on(async move {
        let done = Arc::new(AtomicUsize::new(0));
        let checksum = Arc::new(AtomicU64::new(0));

        for (i, tx) in senders {
            let done_tx = done.clone();
            let checksum_tx = checksum.clone();
            spawner.spawn_detached(async move {
                for _ in 0..sender_yields {
                    boxed_yield_now().await;
                }
                let value = i as u64;
                let _ = tx.send(value);
                checksum_tx.fetch_add(value, Ordering::Relaxed);
                done_tx.fetch_add(1, Ordering::Relaxed);
            });

            if burst != 0 && ((i + 1) % burst) == 0 {
                boxed_yield_now().await;
            }
        }

        while done.load(Ordering::Relaxed) != sender_count {
            boxed_yield_now().await;
        }

        checksum.load(Ordering::Relaxed)
    });
    (checksum, start.elapsed())
}

fn run_bump_sender_shard(
    senders: Vec<(usize, Sender<u64>)>,
    burst: usize,
    sender_yields: usize,
    barrier: Arc<Barrier>,
    shard_id: usize,
) -> (u64, Duration) {
    pin_to(shard_id);
    let sender_count = senders.len();
    let capacity =
        env_usize("FLOODER_ENGINE_CAPACITY", sender_count.saturating_add(1024).max(4096));
    let engine = LocalBumpAsyncEngine::with_task_capacity(capacity);
    let spawner = engine.spawner();
    barrier.wait();
    let start = Instant::now();
    let checksum = engine.block_on(async move {
        let done = Arc::new(AtomicUsize::new(0));
        let checksum = Arc::new(AtomicU64::new(0));

        for (i, tx) in senders {
            let done_tx = done.clone();
            let checksum_tx = checksum.clone();
            spawner.spawn_detached(async move {
                for _ in 0..sender_yields {
                    bump_yield_now().await;
                }
                let value = i as u64;
                let _ = tx.send(value);
                checksum_tx.fetch_add(value, Ordering::Relaxed);
                done_tx.fetch_add(1, Ordering::Relaxed);
            });

            if burst != 0 && ((i + 1) % burst) == 0 {
                bump_yield_now().await;
            }
        }

        while done.load(Ordering::Relaxed) != sender_count {
            bump_yield_now().await;
        }

        checksum.load(Ordering::Relaxed)
    });
    (checksum, start.elapsed())
}

fn run_boxed_receiver_shard(
    receivers: Vec<Receiver<u64>>,
    burst: usize,
    barrier: Arc<Barrier>,
    shard_id: usize,
) -> (u64, Duration) {
    pin_to(shard_id);
    let receiver_count = receivers.len();
    let capacity =
        env_usize("FLOODER_ENGINE_CAPACITY", receiver_count.saturating_add(1024).max(4096));
    let engine = LocalAsyncEngine::with_task_capacity(capacity);
    let spawner = engine.spawner();
    barrier.wait();
    let start = Instant::now();
    let checksum = engine.block_on(async move {
        let done = Arc::new(AtomicUsize::new(0));
        let checksum = Arc::new(AtomicU64::new(0));

        for (i, rx) in receivers.into_iter().enumerate() {
            let done_rx = done.clone();
            let checksum_rx = checksum.clone();
            spawner.spawn_send_detached(async move {
                let value = rx.await.expect("boxed receiver");
                checksum_rx.fetch_add(value, Ordering::Relaxed);
                done_rx.fetch_add(1, Ordering::Relaxed);
            });

            if burst != 0 && ((i + 1) % burst) == 0 {
                boxed_yield_now().await;
            }
        }

        while done.load(Ordering::Relaxed) != receiver_count {
            boxed_yield_now().await;
        }

        checksum.load(Ordering::Relaxed)
    });
    (checksum, start.elapsed())
}

fn run_bump_receiver_shard(
    receivers: Vec<Receiver<u64>>,
    burst: usize,
    barrier: Arc<Barrier>,
    shard_id: usize,
) -> (u64, Duration) {
    pin_to(shard_id);
    let receiver_count = receivers.len();
    let capacity =
        env_usize("FLOODER_ENGINE_CAPACITY", receiver_count.saturating_add(1024).max(4096));
    let engine = LocalBumpAsyncEngine::with_task_capacity(capacity);
    let spawner = engine.spawner();
    barrier.wait();
    let start = Instant::now();
    let checksum = engine.block_on(async move {
        let done = Arc::new(AtomicUsize::new(0));
        let checksum = Arc::new(AtomicU64::new(0));

        for (i, rx) in receivers.into_iter().enumerate() {
            let done_rx = done.clone();
            let checksum_rx = checksum.clone();
            spawner.spawn_send_detached(async move {
                let value = rx.await.expect("bump receiver");
                checksum_rx.fetch_add(value, Ordering::Relaxed);
                done_rx.fetch_add(1, Ordering::Relaxed);
            });

            if burst != 0 && ((i + 1) % burst) == 0 {
                bump_yield_now().await;
            }
        }

        while done.load(Ordering::Relaxed) != receiver_count {
            bump_yield_now().await;
        }

        checksum.load(Ordering::Relaxed)
    });
    (checksum, start.elapsed())
}

fn run_two_shard(
    kind: ReactorKind,
    pairs: usize,
    burst: usize,
    sender_yields: usize,
) -> (u64, Duration) {
    let mut senders = Vec::with_capacity(pairs);
    let mut receivers = Vec::with_capacity(pairs);
    for i in 0..pairs {
        let (tx, rx) = channel::<u64>();
        senders.push((i, tx));
        receivers.push(rx);
    }

    let barrier = Arc::new(Barrier::new(2));
    let sender_barrier = barrier.clone();
    let receiver_barrier = barrier.clone();

    let sender = thread::spawn(move || match kind {
        ReactorKind::Boxed => {
            run_boxed_sender_shard(senders, burst, sender_yields, sender_barrier, 0)
        }
        ReactorKind::Bump => {
            run_bump_sender_shard(senders, burst, sender_yields, sender_barrier, 0)
        }
    });

    let receiver = thread::spawn(move || match kind {
        ReactorKind::Boxed => run_boxed_receiver_shard(receivers, burst, receiver_barrier, 1),
        ReactorKind::Bump => run_bump_receiver_shard(receivers, burst, receiver_barrier, 1),
    });

    let (sender_checksum, sender_elapsed) = sender.join().expect("sender shard");
    let (receiver_checksum, receiver_elapsed) = receiver.join().expect("receiver shard");
    (
        sender_checksum ^ receiver_checksum,
        sender_elapsed.max(receiver_elapsed),
    )
}

fn run_boxed_direct_receiver_shard(
    receivers: Vec<Receiver<()>>,
    barrier: Arc<Barrier>,
    shard_id: usize,
) -> (u64, Duration) {
    pin_to(shard_id);
    let count = receivers.len();
    let capacity = env_usize("FLOODER_ENGINE_CAPACITY", count.saturating_add(16).max(4096));
    let engine = LocalAsyncEngine::with_task_capacity(capacity);
    barrier.wait();
    let start = Instant::now();
    let checksum = engine.block_on(async move {
        for rx in receivers {
            rx.await.expect("boxed direct recv");
        }
        0
    });
    (checksum, start.elapsed())
}

fn run_bump_direct_receiver_shard(
    receivers: Vec<Receiver<()>>,
    barrier: Arc<Barrier>,
    shard_id: usize,
) -> (u64, Duration) {
    pin_to(shard_id);
    barrier.wait();
    let start = Instant::now();
    let mut polls = 0u64;
    for rx in receivers {
        polls += 1;
        match drive_future_to_completion(async move { rx.await }) {
            Ok(()) => {}
            Err(_) => panic!("bump direct recv failed"),
        }
    }
    (polls, start.elapsed())
}

fn run_direct_two_shard(kind: ReactorKind, pairs: usize) -> (u64, Duration) {
    let mut senders = Vec::with_capacity(pairs);
    let mut receivers = Vec::with_capacity(pairs);
    for i in 0..pairs {
        let (tx, rx) = channel::<()>();
        senders.push((i, tx));
        receivers.push(rx);
    }

    let barrier = Arc::new(Barrier::new(2));
    let sender_barrier = barrier.clone();
    let receiver_barrier = barrier.clone();

    let sender = thread::spawn(move || {
        pin_to(0);
        sender_barrier.wait();
        let start = Instant::now();
        for (_, tx) in senders {
            let _ = tx.send(());
        }
        (0, start.elapsed())
    });

    let receiver = thread::spawn(move || match kind {
        ReactorKind::Boxed => run_boxed_direct_receiver_shard(receivers, receiver_barrier, 1),
        ReactorKind::Bump => run_bump_direct_receiver_shard(receivers, receiver_barrier, 1),
    });

    let sender_checksum = sender.join().expect("sender shard").0;
    let (receiver_checksum, receiver_elapsed) = receiver.join().expect("receiver shard");
    (sender_checksum ^ receiver_checksum, receiver_elapsed)
}

fn run_direct_two_shard_native(pairs: usize) -> (u64, Duration) {
    let mut senders = Vec::with_capacity(pairs);
    let mut receivers = Vec::with_capacity(pairs);
    for i in 0..pairs {
        let (tx, rx) = channel::<()>();
        senders.push((i, tx));
        receivers.push(rx);
    }

    let barrier = Arc::new(Barrier::new(2));
    let sender_barrier = barrier.clone();
    let receiver_barrier = barrier.clone();

    let sender = thread::spawn(move || {
        pin_to(0);
        sender_barrier.wait();
        let start = Instant::now();
        for (_, tx) in senders {
            let _ = tx.send(());
        }
        (0, start.elapsed())
    });

    let receiver = thread::spawn(move || {
        pin_to(1);
        let capacity = env_usize("FLOODER_ENGINE_CAPACITY", pairs.saturating_add(16).max(4096));
        let hot = env_usize("ONESHOT_HOT", 1);
        let budget = env_usize("ONESHOT_BUDGET", 4096);
        let mut engine = NativeTaskEngine::with_task_capacity(capacity);
        let polls = Arc::new(AtomicUsize::new(0));
        engine.spawn(CountedNativeFutureTask::ready(
            async move {
                for rx in receivers {
                    rx.await.expect("tokio async recv");
                }
            },
            polls.clone(),
        ));
        receiver_barrier.wait();
        let start = Instant::now();
        while engine.active_tasks() != 0 {
            let progressed = engine.run_hot_budget_dynamic(hot, budget);
            if progressed == 0 {
                thread::yield_now();
            }
        }
        (polls.load(Ordering::Relaxed) as u64, start.elapsed())
    });

    let sender_checksum = sender.join().expect("sender shard").0;
    let (receiver_checksum, receiver_elapsed) = receiver.join().expect("receiver shard");
    (sender_checksum ^ receiver_checksum, receiver_elapsed)
}

fn run_queue_two_shard_bump(pairs: usize, burst: usize, sender_yields: usize) -> (u64, Duration) {
    let global = Arc::new(GlobalContext::new(2));
    let barrier = Arc::new(Barrier::new(2));
    let owner_handle = global.shard_handle(0);
    let mut senders = Vec::with_capacity(pairs);
    let mut receivers = Vec::with_capacity(pairs);
    for _ in 0..pairs {
        let (sender, receiver) = owner_handle
            .oneshot_any::<()>()
            .expect("queue oneshot allocation");
        senders.push(sender);
        receivers.push(receiver);
    }

    let owner_barrier = barrier.clone();
    let owner = thread::spawn(move || {
        pin_to(0);
        let capacity = env_usize("FLOODER_ENGINE_CAPACITY", pairs.saturating_add(16).max(4096));
        let engine = LocalBumpAsyncEngine::with_task_capacity(capacity);
        let spawner = engine.spawner();
        let completed = Arc::new(AtomicUsize::new(0));
        owner_barrier.wait();
        let start = Instant::now();

        spawner.spawn_send_detached({
            let completed = completed.clone();
            async move {
                for receiver in receivers {
                    receiver.await.expect("queue oneshot recv");
                    completed.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        loop {
            let progressed = engine.run_until_stalled();
            if completed.load(Ordering::Acquire) == pairs {
                let _ = engine.run_until_stalled();
                break (0, start.elapsed());
            }
            if progressed == 0 {
                thread::yield_now();
            }
        }
    });

    let sender = thread::spawn(move || {
        pin_to(1);
        barrier.wait();
        let start = Instant::now();
        for (i, sender) in senders.into_iter().enumerate() {
            for _ in 0..sender_yields {
                std::hint::spin_loop();
            }
            let _ = sender.send(());
            if burst != 0 && ((i + 1) % burst) == 0 {
                std::hint::spin_loop();
            }
        }
        (0, start.elapsed())
    });

    let (recv_checksum, recv_elapsed) = owner.join().expect("owner shard");
    let sent_checksum = sender.join().expect("sender shard").0;
    (recv_checksum ^ sent_checksum, recv_elapsed)
}

fn run_queue_two_shard_fast(pairs: usize, burst: usize, sender_yields: usize) -> (u64, Duration) {
    let global = Arc::new(GlobalContext::new(2));
    let barrier = Arc::new(Barrier::new(2));
    let owner_handle = global.shard_handle(0);
    let mut senders = Vec::with_capacity(pairs);
    let mut receivers = Vec::with_capacity(pairs);
    for _ in 0..pairs {
        let (sender, receiver) = owner_handle
            .oneshot_any::<()>()
            .expect("queue oneshot allocation");
        senders.push(sender);
        receivers.push(receiver);
    }

    let owner_barrier = barrier.clone();
    let owner = thread::spawn(move || {
        pin_to(0);
        owner_barrier.wait();
        let start = Instant::now();
        let mut polls = 0u64;
        for receiver in receivers {
            polls += 1;
            match drive_future_to_completion(receiver) {
                Ok(()) => {}
                Err(_) => panic!("queue fast recv failed"),
            }
        }
        (polls, start.elapsed())
    });

    let sender = thread::spawn(move || {
        pin_to(1);
        barrier.wait();
        let start = Instant::now();
        for (i, sender) in senders.into_iter().enumerate() {
            for _ in 0..sender_yields {
                std::hint::spin_loop();
            }
            let _ = sender.send(());
            if burst != 0 && ((i + 1) % burst) == 0 {
                std::hint::spin_loop();
            }
        }
        (0, start.elapsed())
    });

    let (recv_checksum, recv_elapsed) = owner.join().expect("owner shard");
    let sent_checksum = sender.join().expect("sender shard").0;
    (recv_checksum ^ sent_checksum, recv_elapsed)
}

fn main() {
    let kind = ReactorKind::from_env();
    let mode = Mode::from_env();
    let pairs = env_usize("ONESHOT_PAIRS", 1_000_000);
    let burst = env_usize("ONESHOT_BURST", 64);
    let sender_yields = env_usize("ONESHOT_SENDER_YIELDS", 0);

    let start = Instant::now();
    let (metric, elapsed) = match mode {
        Mode::Local => {
            let checksum = match kind {
                ReactorKind::Boxed => run_boxed_local(pairs, burst, sender_yields),
                ReactorKind::Bump => run_bump_local(pairs, burst, sender_yields),
            };
            (checksum, start.elapsed())
        }
        Mode::TwoShard => run_two_shard(kind, pairs, burst, sender_yields),
        Mode::DirectTwoShard => run_direct_two_shard(kind, pairs),
        Mode::DirectTwoShardNative => match kind {
            ReactorKind::Bump => run_direct_two_shard_native(pairs),
            ReactorKind::Boxed => panic!("direct_two_shard_native is only implemented for bump"),
        },
        Mode::QueueTwoShard => match kind {
            ReactorKind::Bump => run_queue_two_shard_bump(pairs, burst, sender_yields),
            ReactorKind::Boxed => panic!("queue_two_shard is only implemented for bump"),
        },
        Mode::QueueTwoShardFast => match kind {
            ReactorKind::Bump => run_queue_two_shard_fast(pairs, burst, sender_yields),
            ReactorKind::Boxed => panic!("queue_two_shard_fast is only implemented for bump"),
        },
    };

    let msgs_per_sec = pairs as f64 / elapsed.as_secs_f64();
    let polls_per_msg = metric as f64 / pairs as f64;
    println!(
        "reactor_oneshot_bench reactor={:?} mode={:?} pairs={} burst={} sender_yields={} elapsed_ms={:.3} msgs_per_sec={:.0} polls={} polls_per_msg={:.3}",
        kind,
        mode,
        pairs,
        burst,
        sender_yields,
        elapsed.as_secs_f64() * 1000.0,
        msgs_per_sec,
        metric,
        polls_per_msg
    );
}
