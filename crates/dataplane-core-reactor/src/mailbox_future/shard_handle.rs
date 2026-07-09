use crate::mailbox_future::global_context::GlobalContext;
use crate::mailbox_future::remote_task::{
    poll_boxed_runtime_future_once, BoxedRuntimeFuture, QueuedRuntimeFuture, RemoteTaskReceiver,
    RemoteValueSender, RuntimeFutureTask, ShardMessage, ShardRuntimeQueues, REMOTE_DRAIN_BURST,
};
use crate::mailbox_future::result_cell::{RemoteTaskResult, RemoteWaitCapacityError};
use crate::mailbox_future::signal::SignalEntry;
use crate::native_task::NativeTaskEngine;
use rtrb::chunks::ChunkError;
use std::cell::RefCell;
use std::future::Future;
use std::sync::Arc;
use std::task::Poll;

#[derive(Debug, PartialEq, Eq)]
pub enum RemoteSpawnError {
    WaitCapacity(RemoteWaitCapacityError),
    QueueFull { target_shard: usize },
}

impl From<RemoteWaitCapacityError> for RemoteSpawnError {
    #[inline(always)]
    fn from(value: RemoteWaitCapacityError) -> Self {
        Self::WaitCapacity(value)
    }
}

pub struct ShardRuntimeHandle {
    global: Arc<GlobalContext>,
    current_shard: usize,
    runtime: RefCell<ShardRuntimeQueues>,
    outbound_pending: RefCell<Vec<Vec<QueuedRuntimeFuture>>>,
}

impl ShardRuntimeHandle {
    #[inline(always)]
    pub(crate) fn new(
        global: Arc<GlobalContext>,
        current_shard: usize,
        runtime: ShardRuntimeQueues,
        shard_count: usize,
    ) -> Self {
        Self {
            global,
            current_shard,
            runtime: RefCell::new(runtime),
            outbound_pending: RefCell::new((0..shard_count).map(|_| Vec::new()).collect()),
        }
    }

    #[inline(always)]
    pub fn current_shard(&self) -> usize {
        self.current_shard
    }

    fn push_runtime_task(
        &self,
        target_shard: usize,
        queued: QueuedRuntimeFuture,
    ) -> Result<(), QueuedRuntimeFuture> {
        debug_assert_ne!(target_shard, self.current_shard);
        self.flush_runtime_tasks();
        let mut pending = self.outbound_pending.borrow_mut();
        let queue = &mut pending[target_shard];
        if queue.len() >= self.global.runtime_queue_capacity() {
            return Err(queued);
        }
        queue.push(queued);
        Ok(())
    }

    pub fn flush_runtime_tasks(&self) -> usize {
        let mut flushed = 0usize;
        let mut pending = self.outbound_pending.borrow_mut();
        for target_shard in 0..pending.len() {
            if target_shard == self.current_shard || pending[target_shard].is_empty() {
                continue;
            }
            let queue = &mut pending[target_shard];
            let mut producer = self.global.runtime_outbound[self.current_shard][target_shard]
                .lock()
                .expect("outbound runtime queue mutex poisoned");
            let producer = producer
                .as_mut()
                .expect("source and target shard must differ");
            while !queue.is_empty() {
                let slots = producer.cached_slots().max(producer.slots());
                if slots == 0 {
                    break;
                }
                let burst = slots.min(queue.len()).min(REMOTE_DRAIN_BURST);
                let chunk = match producer.write_chunk_uninit(burst) {
                    Ok(chunk) => chunk,
                    Err(ChunkError::TooFewSlots(_)) => break,
                };
                let wrote = chunk.fill_from_iter(queue.drain(..burst).map(ShardMessage::Runtime));
                debug_assert_eq!(wrote, burst);
                flushed += wrote;
            }
        }
        flushed
    }

    fn oneshot_reserved_at<T>(&self, index: u16) -> (RemoteValueSender<T>, RemoteTaskReceiver<T>)
    where
        T: Send + 'static,
    {
        let target_shard = self.global.target_shard_for_index(index);
        assert_ne!(
            target_shard, self.current_shard,
            "oneshot_at requires a remote target shard; use a local channel for local tasks"
        );
        let signal = Arc::new(SignalEntry::new());
        let signal_key = self.global.register_signal(signal);
        let result = Arc::new(RemoteTaskResult::<T>::new(self.current_shard));
        let receiver = RemoteTaskReceiver {
            global: self.global.clone(),
            signal: Some(signal_key),
            result: result.clone(),
            owner_shard: Some(self.current_shard),
            _marker: std::marker::PhantomData,
        };
        let sender = RemoteValueSender::new(
            self.global.clone(),
            Some(signal_key),
            result,
            Some(self.current_shard),
        );
        (sender, receiver)
    }

    fn spawn_reserved_at<F, T>(
        &self,
        index: u16,
        future: F,
    ) -> Result<RemoteTaskReceiver<T>, RemoteSpawnError>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let target_shard = self.global.target_shard_for_index(index);
        assert_ne!(
            target_shard, self.current_shard,
            "spawn_at requires a remote target shard; use the local scheduler for local tasks"
        );
        let (publisher, receiver) = self.oneshot_reserved_at(index);
        let task: BoxedRuntimeFuture = Box::pin(async move {
            let output = future.await;
            let _ = publisher.send(output);
        });
        self.push_runtime_task(target_shard, QueuedRuntimeFuture::new(task))
            .map_err(|_| RemoteSpawnError::QueueFull { target_shard })?;
        Ok(receiver)
    }

    fn drain_runtime_queue_inner<F>(&self, mut f: F) -> usize
    where
        F: FnMut(BoxedRuntimeFuture),
    {
        let _ = self.flush_runtime_tasks();
        let mut drained = 0usize;
        let mut runtime = self.runtime.borrow_mut();
        for consumer_cell in &mut runtime.inbound {
            let Some(consumer) = consumer_cell.as_mut() else {
                continue;
            };
            loop {
                let slots = consumer.cached_slots().max(consumer.slots());
                if slots == 0 {
                    break;
                }
                let burst = slots.min(REMOTE_DRAIN_BURST);
                let chunk = match consumer.read_chunk(burst) {
                    Ok(chunk) => chunk,
                    Err(ChunkError::TooFewSlots(0)) => break,
                    Err(ChunkError::TooFewSlots(_)) => {
                        std::hint::spin_loop();
                        continue;
                    }
                };
                for message in chunk.into_iter() {
                    let ShardMessage::Runtime(queued) = message;
                    let future = queued.into_future();
                    f(future);
                    drained += 1;
                }
            }
        }
        drained
    }

    #[inline(always)]
    pub fn spawn_at<F, T>(
        &self,
        index: u16,
        future: F,
    ) -> Result<RemoteTaskReceiver<T>, RemoteSpawnError>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.spawn_reserved_at(index, future)
    }

    #[inline(always)]
    pub fn spawn_any<F, T>(&self, future: F) -> Result<RemoteTaskReceiver<T>, RemoteSpawnError>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let index = self.global.allocate_remote_index(self.current_shard)?;
        self.spawn_reserved_at(index, future)
    }

    #[inline(always)]
    pub fn oneshot_at<T>(&self, index: u16) -> (RemoteValueSender<T>, RemoteTaskReceiver<T>)
    where
        T: Send + 'static,
    {
        self.oneshot_reserved_at(index)
    }

    #[inline(always)]
    pub fn oneshot_any<T>(
        &self,
    ) -> Result<(RemoteValueSender<T>, RemoteTaskReceiver<T>), RemoteWaitCapacityError>
    where
        T: Send + 'static,
    {
        let index = self.global.allocate_remote_index(self.current_shard)?;
        Ok(self.oneshot_reserved_at(index))
    }

    #[inline(always)]
    pub fn spawn_at_detached<F>(&self, index: u16, future: F) -> Result<(), RemoteSpawnError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let target_shard = self.global.target_shard_for_index(index);
        assert_ne!(
            target_shard, self.current_shard,
            "spawn_at_detached requires a remote target shard; use the local scheduler for local tasks"
        );
        self.push_runtime_task(target_shard, QueuedRuntimeFuture::new(Box::pin(future)))
            .map_err(|_| RemoteSpawnError::QueueFull { target_shard })
    }

    #[inline(always)]
    pub fn spawn_any_detached<F>(&self, future: F) -> Result<(), RemoteSpawnError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let target_shard = self.global.choose_remote_shard(self.current_shard);
        self.push_runtime_task(target_shard, QueuedRuntimeFuture::new(Box::pin(future)))
            .map_err(|_| RemoteSpawnError::QueueFull { target_shard })
    }

    #[inline(always)]
    pub fn drain_native_runtime_queue(
        &self,
        engine: &mut NativeTaskEngine<RuntimeFutureTask>,
    ) -> usize {
        let mut drained = 0usize;
        self.drain_runtime_queue_inner(|future| {
            let mut future = future;
            if matches!(poll_boxed_runtime_future_once(&mut future), Poll::Pending) {
                engine.spawn(RuntimeFutureTask::parked(future));
            }
            drained += 1;
        });
        drained
    }

    #[inline(always)]
    pub fn drain_runtime_queue_with<F>(&self, f: F) -> usize
    where
        F: FnMut(BoxedRuntimeFuture),
    {
        self.drain_runtime_queue_inner(f)
    }
}
