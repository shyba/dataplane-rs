use super::{FocusPolicy, LocalMeshScheduler, TaskCell};

pub trait PushPolicy {
    fn try_offload<Op, const STACK_BYTES: usize, Focus>(
        scheduler: &mut LocalMeshScheduler<Op, STACK_BYTES, Focus, Self>,
        mut task: TaskCell<Op, STACK_BYTES>,
    ) -> Result<usize, TaskCell<Op, STACK_BYTES>>
    where
        Focus: FocusPolicy,
        Self: Sized,
    {
        let route_len = scheduler.route().len();
        let mut i = 0usize;
        while i < route_len {
            let dst = scheduler.route()[i];
            i += 1;
            if dst == scheduler.shard_id() {
                continue;
            }
            if !scheduler.scope_allows_destination(
                task.meta.scope,
                task.meta.origin_shard,
                task.meta.origin_domain,
                dst,
            ) {
                continue;
            }
            match scheduler.mesh_try_push(dst, task) {
                Ok(()) => return Ok(dst),
                Err(returned) => task = returned,
            }
        }
        Err(task)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TopologyRoutePush;

impl PushPolicy for TopologyRoutePush {}
