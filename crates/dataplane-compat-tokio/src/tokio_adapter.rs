use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use dataplane_compat::boundary::{BatchReply, BoundaryAdapter, SessionBatch};
use dataplane_runtime::errors::{NifError, Result};
use tokio::sync::oneshot;

type BoxReplyFuture = Pin<Box<dyn Future<Output = Result<BatchReply>> + Send + 'static>>;

pub trait TokioBatchSink: Send + Sync + 'static {
    fn submit_batch(
        &self,
        batch: SessionBatch,
        reply: oneshot::Sender<Result<BatchReply>>,
    ) -> Result<()>;
}

#[derive(Clone)]
pub struct TokioBoundaryAdapter<S> {
    sink: Arc<S>,
}

impl<S> TokioBoundaryAdapter<S> {
    pub fn new(sink: Arc<S>) -> Self {
        Self { sink }
    }
}

impl<S> BoundaryAdapter for TokioBoundaryAdapter<S>
where
    S: TokioBatchSink,
{
    type ReplyFuture = BoxReplyFuture;

    fn submit_batch(&self, batch: SessionBatch) -> Self::ReplyFuture {
        let sink = Arc::clone(&self.sink);
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            sink.submit_batch(batch, tx)?;
            rx.await.map_err(|_| NifError::Closed)?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dataplane_runtime::runtime_protocol::BatchOp;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct ImmediateSink {
        calls: AtomicUsize,
    }

    impl TokioBatchSink for ImmediateSink {
        fn submit_batch(
            &self,
            batch: SessionBatch,
            reply: oneshot::Sender<Result<BatchReply>>,
        ) -> Result<()> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let _ = reply.send(Ok(BatchReply {
                session_id: batch.session_id,
                results: Vec::new(),
            }));
            Ok(())
        }
    }

    struct ClosedSink;

    impl TokioBatchSink for ClosedSink {
        fn submit_batch(
            &self,
            _batch: SessionBatch,
            _reply: oneshot::Sender<Result<BatchReply>>,
        ) -> Result<()> {
            Ok(())
        }
    }

    fn test_batch() -> SessionBatch {
        SessionBatch {
            session_id: 7,
            ops: vec![BatchOp::Read { id: 1, len: 16 }],
        }
    }

    #[test]
    fn tokio_boundary_adapter_forwards_reply_without_spawn_blocking() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");
        let sink = Arc::new(ImmediateSink {
            calls: AtomicUsize::new(0),
        });
        let adapter = TokioBoundaryAdapter::new(Arc::clone(&sink));

        let reply = runtime
            .block_on(adapter.submit_batch(test_batch()))
            .expect("adapter reply");

        assert_eq!(reply.session_id, 7);
        assert!(reply.results.is_empty());
        assert_eq!(sink.calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn tokio_boundary_adapter_returns_closed_if_reply_sender_drops() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");
        let adapter = TokioBoundaryAdapter::new(Arc::new(ClosedSink));

        let err = match runtime.block_on(adapter.submit_batch(test_batch())) {
            Ok(_) => panic!("reply channel should close"),
            Err(err) => err,
        };

        assert!(matches!(err, NifError::Closed));
    }
}
