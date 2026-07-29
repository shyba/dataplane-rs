use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use dataplane_compat::boundary::{
    BatchReply, BoundaryAdapter, BoundaryError, BoundaryResult, SessionBatch,
};
use tokio::sync::oneshot;

type BoxReplyFuture = Pin<Box<dyn Future<Output = BoundaryResult<BatchReply>> + Send + 'static>>;

pub trait TokioBatchSink: Send + Sync + 'static {
    fn submit_batch(
        &self,
        batch: SessionBatch,
        reply: oneshot::Sender<BoundaryResult<BatchReply>>,
    ) -> BoundaryResult<()>;
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
            rx.await.map_err(|_| BoundaryError::Closed)?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dataplane_compat::boundary::{
        validate_reply, BatchConformanceError, BatchOp, BatchResult,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct ImmediateSink {
        calls: AtomicUsize,
    }

    impl TokioBatchSink for ImmediateSink {
        fn submit_batch(
            &self,
            batch: SessionBatch,
            reply: oneshot::Sender<BoundaryResult<BatchReply>>,
        ) -> BoundaryResult<()> {
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
            _reply: oneshot::Sender<BoundaryResult<BatchReply>>,
        ) -> BoundaryResult<()> {
            Ok(())
        }
    }

    /// A sink that answers every op with a kind-correct, id-matching result.
    struct ConformingSink;

    impl TokioBatchSink for ConformingSink {
        fn submit_batch(
            &self,
            batch: SessionBatch,
            reply: oneshot::Sender<BoundaryResult<BatchReply>>,
        ) -> BoundaryResult<()> {
            let results = batch
                .ops
                .iter()
                .map(|op| match op {
                    BatchOp::Read { id, len } => (*id, BatchResult::Data(vec![0u8; *len])),
                    BatchOp::Write { id, .. } => (*id, BatchResult::Ok),
                })
                .collect();
            let _ = reply.send(Ok(BatchReply {
                session_id: batch.session_id,
                results,
            }));
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
    fn conformance_harness_accepts_a_conforming_adapter() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");
        let adapter = TokioBoundaryAdapter::new(Arc::new(ConformingSink));

        let batch = test_batch();
        let expected_session = batch.session_id;
        let expected_ops = batch.ops.clone();

        let reply = runtime
            .block_on(adapter.submit_batch(batch))
            .expect("adapter reply");

        let expected = SessionBatch {
            session_id: expected_session,
            ops: expected_ops,
        };
        assert_eq!(validate_reply(&expected, &reply), Ok(()));
    }

    #[test]
    fn conformance_harness_catches_empty_reply_from_immediate_sink() {
        // ImmediateSink replies with no results, which silently violates the contract
        // for a batch that submitted a Read. The harness surfaces it.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");
        let adapter = TokioBoundaryAdapter::new(Arc::new(ImmediateSink {
            calls: AtomicUsize::new(0),
        }));

        let batch = test_batch();
        let expected_ops = batch.ops.clone();

        let reply = runtime
            .block_on(adapter.submit_batch(batch))
            .expect("adapter reply");

        let expected = SessionBatch {
            session_id: 7,
            ops: expected_ops,
        };
        assert_eq!(
            validate_reply(&expected, &reply),
            Err(BatchConformanceError::MissingResult(1))
        );
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

        assert!(matches!(err, BoundaryError::Closed));
    }
}
