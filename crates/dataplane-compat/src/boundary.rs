use std::future::Future;

pub type BoundaryResult<T> = std::result::Result<T, BoundaryError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryError {
    Errno(i32),
    Closed,
    Timeout,
    StartupProfileLayout,
}

impl BoundaryError {
    pub fn from_errno(errno: i32) -> Self {
        Self::Errno(errno)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchOp {
    Read { id: u64, len: usize },
    Write { id: u64, data: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchResult {
    Ok,
    Data(Vec<u8>),
    Error(BoundaryError),
}

pub struct SessionBatch {
    pub session_id: u64,
    pub ops: Vec<BatchOp>,
}

pub struct BatchReply {
    pub session_id: u64,
    pub results: Vec<(u64, BatchResult)>,
}

pub trait BoundaryAdapter {
    type ReplyFuture: Future<Output = BoundaryResult<BatchReply>> + Send;

    fn submit_batch(&self, batch: SessionBatch) -> Self::ReplyFuture;
}

/// Compile-time surface guard for the `submit_batch` BoundaryAdapter method.
///
/// The dummy implementation below only implements `submit_batch`. If a second
/// required method is ever added to BoundaryAdapter, the dummy fails to
/// compile until the new surface is reviewed. This does not prove
/// `submit_batch` is the only required method—only that adding another
/// required method will break the compile guard.
#[cfg(test)]
mod surface_guard {
    use super::*;

    struct DummyAdapter;

    impl BoundaryAdapter for DummyAdapter {
        type ReplyFuture = std::future::Ready<BoundaryResult<BatchReply>>;

        fn submit_batch(&self, _batch: SessionBatch) -> Self::ReplyFuture {
            std::future::ready(Ok(BatchReply {
                session_id: 0,
                results: Vec::new(),
            }))
        }
    }

    #[test]
    fn boundary_adapter_submit_batch_guard() {
        fn assert_surface<A: BoundaryAdapter>(_: &A) {}
        let adapter = DummyAdapter;
        assert_surface(&adapter);
    }
}
