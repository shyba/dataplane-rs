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

impl BatchOp {
    /// The correlation id an adapter must echo back in the matching result.
    pub fn id(&self) -> u64 {
        match self {
            BatchOp::Read { id, .. } | BatchOp::Write { id, .. } => *id,
        }
    }
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

/// A way a [`BatchReply`] can violate the boundary contract for its [`SessionBatch`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchConformanceError {
    /// The reply's `session_id` does not echo the batch's.
    SessionMismatch { expected: u64, actual: u64 },
    /// Two ops in the batch share a correlation id, so results cannot correspond.
    DuplicateOpId(u64),
    /// The reply lists the same result id twice.
    DuplicateResultId(u64),
    /// A submitted op received no result.
    MissingResult(u64),
    /// A result carries an id that was never submitted.
    UnknownResultId(u64),
    /// The result kind is impossible for the op kind (e.g. `Data` for a `Write`,
    /// or `Ok` for a `Read`). `Error` is always permitted.
    ResultKindMismatch(u64),
}

impl std::fmt::Display for BatchConformanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionMismatch { expected, actual } => {
                write!(f, "session id mismatch: expected {expected}, got {actual}")
            }
            Self::DuplicateOpId(id) => write!(f, "duplicate op id {id} in batch"),
            Self::DuplicateResultId(id) => write!(f, "duplicate result id {id} in reply"),
            Self::MissingResult(id) => write!(f, "op {id} has no result"),
            Self::UnknownResultId(id) => write!(f, "result id {id} was never submitted"),
            Self::ResultKindMismatch(id) => write!(f, "result kind invalid for op {id}"),
        }
    }
}

impl std::error::Error for BatchConformanceError {}

fn result_matches_op(op: &BatchOp, result: &BatchResult) -> bool {
    match (op, result) {
        (_, BatchResult::Error(_)) => true,
        (BatchOp::Read { .. }, BatchResult::Data(_)) => true,
        (BatchOp::Write { .. }, BatchResult::Ok) => true,
        _ => false,
    }
}

/// Check a reply against the batch it answers, enforcing the boundary contract that
/// the types only imply: the session id is echoed, every op has exactly one result
/// with a corresponding id, no result is unknown or duplicated, and each result kind
/// is valid for its op kind.
///
/// Cheap (`O(ops + results)`); call it on the reply path in debug builds, and drive
/// adapter test suites through it as the conformance harness.
pub fn validate_reply(
    batch: &SessionBatch,
    reply: &BatchReply,
) -> Result<(), BatchConformanceError> {
    use std::collections::{HashMap, HashSet};

    if reply.session_id != batch.session_id {
        return Err(BatchConformanceError::SessionMismatch {
            expected: batch.session_id,
            actual: reply.session_id,
        });
    }

    let mut ops_by_id: HashMap<u64, &BatchOp> = HashMap::with_capacity(batch.ops.len());
    for op in &batch.ops {
        if ops_by_id.insert(op.id(), op).is_some() {
            return Err(BatchConformanceError::DuplicateOpId(op.id()));
        }
    }

    let mut seen: HashSet<u64> = HashSet::with_capacity(reply.results.len());
    for (id, result) in &reply.results {
        if !seen.insert(*id) {
            return Err(BatchConformanceError::DuplicateResultId(*id));
        }
        let Some(op) = ops_by_id.get(id) else {
            return Err(BatchConformanceError::UnknownResultId(*id));
        };
        if !result_matches_op(op, result) {
            return Err(BatchConformanceError::ResultKindMismatch(*id));
        }
    }

    for op in &batch.ops {
        if !seen.contains(&op.id()) {
            return Err(BatchConformanceError::MissingResult(op.id()));
        }
    }

    Ok(())
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

#[cfg(test)]
mod conformance {
    use super::*;

    fn batch() -> SessionBatch {
        SessionBatch {
            session_id: 42,
            ops: vec![
                BatchOp::Read { id: 1, len: 8 },
                BatchOp::Write { id: 2, data: vec![0xAB] },
            ],
        }
    }

    fn reply(results: Vec<(u64, BatchResult)>) -> BatchReply {
        BatchReply {
            session_id: 42,
            results,
        }
    }

    #[test]
    fn conforming_reply_passes() {
        let ok = reply(vec![
            (1, BatchResult::Data(vec![1, 2, 3])),
            (2, BatchResult::Ok),
        ]);
        assert_eq!(validate_reply(&batch(), &ok), Ok(()));
    }

    #[test]
    fn error_result_is_valid_for_any_op_kind() {
        let ok = reply(vec![
            (1, BatchResult::Error(BoundaryError::Timeout)),
            (2, BatchResult::Error(BoundaryError::Errno(5))),
        ]);
        assert_eq!(validate_reply(&batch(), &ok), Ok(()));
    }

    #[test]
    fn out_of_order_results_pass() {
        let ok = reply(vec![
            (2, BatchResult::Ok),
            (1, BatchResult::Data(vec![9])),
        ]);
        assert_eq!(validate_reply(&batch(), &ok), Ok(()));
    }

    #[test]
    fn session_mismatch_is_rejected() {
        let bad = BatchReply {
            session_id: 99,
            results: vec![(1, BatchResult::Data(vec![1])), (2, BatchResult::Ok)],
        };
        assert_eq!(
            validate_reply(&batch(), &bad),
            Err(BatchConformanceError::SessionMismatch {
                expected: 42,
                actual: 99
            })
        );
    }

    #[test]
    fn missing_result_is_rejected() {
        let bad = reply(vec![(1, BatchResult::Data(vec![1]))]);
        assert_eq!(
            validate_reply(&batch(), &bad),
            Err(BatchConformanceError::MissingResult(2))
        );
    }

    #[test]
    fn empty_reply_for_nonempty_batch_is_rejected() {
        let bad = reply(vec![]);
        assert!(matches!(
            validate_reply(&batch(), &bad),
            Err(BatchConformanceError::MissingResult(_))
        ));
    }

    #[test]
    fn unknown_result_id_is_rejected() {
        let bad = reply(vec![
            (1, BatchResult::Data(vec![1])),
            (2, BatchResult::Ok),
            (3, BatchResult::Ok),
        ]);
        assert_eq!(
            validate_reply(&batch(), &bad),
            Err(BatchConformanceError::UnknownResultId(3))
        );
    }

    #[test]
    fn duplicate_result_id_is_rejected() {
        let bad = reply(vec![
            (1, BatchResult::Data(vec![1])),
            (1, BatchResult::Data(vec![2])),
        ]);
        assert_eq!(
            validate_reply(&batch(), &bad),
            Err(BatchConformanceError::DuplicateResultId(1))
        );
    }

    #[test]
    fn duplicate_op_id_is_rejected() {
        let ambiguous = SessionBatch {
            session_id: 42,
            ops: vec![
                BatchOp::Read { id: 1, len: 8 },
                BatchOp::Read { id: 1, len: 4 },
            ],
        };
        assert_eq!(
            validate_reply(&ambiguous, &reply(vec![(1, BatchResult::Data(vec![1]))])),
            Err(BatchConformanceError::DuplicateOpId(1))
        );
    }

    #[test]
    fn data_result_for_write_is_rejected() {
        let bad = reply(vec![
            (1, BatchResult::Data(vec![1])),
            (2, BatchResult::Data(vec![2])),
        ]);
        assert_eq!(
            validate_reply(&batch(), &bad),
            Err(BatchConformanceError::ResultKindMismatch(2))
        );
    }

    #[test]
    fn ok_result_for_read_is_rejected() {
        let bad = reply(vec![(1, BatchResult::Ok), (2, BatchResult::Ok)]);
        assert_eq!(
            validate_reply(&batch(), &bad),
            Err(BatchConformanceError::ResultKindMismatch(1))
        );
    }
}
