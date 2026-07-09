use dataplane_topology::{
    ShardGroup, ShardPlacement as TopologyShardPlacement, ShardTopology, TopologyStrategy,
};
use rtrb::{Consumer, PopError, Producer, PushError, RingBuffer};
use std::collections::{HashSet, VecDeque};

use super::types::SchedulerPlacement;

struct TxLane<T> {
    producer: Producer<T>,
    spill: VecDeque<T>,
    spill_capacity: usize,
}

impl<T> TxLane<T> {
    #[inline(always)]
    fn flush_spill(&mut self) {
        loop {
            let Some(item) = self.spill.pop_front() else {
                break;
            };
            match self.producer.push(item) {
                Ok(()) => {}
                Err(PushError::Full(item)) => {
                    self.spill.push_front(item);
                    break;
                }
            }
        }
    }

    #[inline(always)]
    fn has_spill_capacity(&self) -> bool {
        self.spill.len() < self.spill_capacity
    }
}

struct RxLane<T> {
    consumer: Consumer<T>,
}

pub struct ShardMeshEndpoint<T> {
    shard_id: usize,
    tx: Vec<Option<TxLane<T>>>,
    rx: Vec<Option<RxLane<T>>>,
    ready_sources: VecDeque<usize>,
    ready_enqueued: Vec<bool>,
    scan_cursor: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ShardMeshPushError<T> {
    LocalShard(T),
    MissingLane(T),
    Full(T),
    Abandoned(T),
}

impl<T> ShardMeshPushError<T> {
    #[inline(always)]
    pub fn into_inner(self) -> T {
        match self {
            Self::LocalShard(item)
            | Self::MissingLane(item)
            | Self::Full(item)
            | Self::Abandoned(item) => item,
        }
    }
}

impl<T> ShardMeshEndpoint<T> {
    #[inline(always)]
    pub fn shard_id(&self) -> usize {
        self.shard_id
    }

    #[inline(always)]
    pub fn shard_count(&self) -> usize {
        self.tx.len()
    }

    pub fn try_push(&mut self, dst: usize, item: T) -> Result<(), T> {
        self.try_push_admit(dst, item)
            .map_err(|err| err.into_inner())
    }

    pub fn try_push_admit(&mut self, dst: usize, item: T) -> Result<(), ShardMeshPushError<T>> {
        if dst == self.shard_id {
            return Err(ShardMeshPushError::LocalShard(item));
        }

        let Some(lane) = self.tx.get_mut(dst).and_then(Option::as_mut) else {
            return Err(ShardMeshPushError::MissingLane(item));
        };

        lane.flush_spill();

        if lane.producer.is_abandoned() {
            return Err(ShardMeshPushError::Abandoned(item));
        }

        match lane.producer.push(item) {
            Ok(()) => Ok(()),
            Err(PushError::Full(item)) => {
                if !lane.has_spill_capacity() {
                    return Err(ShardMeshPushError::Full(item));
                }
                lane.spill.push_back(item);
                Ok(())
            }
        }
    }

    pub fn drain_incoming<F>(
        &mut self,
        max_total: usize,
        scan_budget: usize,
        per_source_budget: usize,
        mut f: F,
    ) -> usize
    where
        F: FnMut(T),
    {
        if max_total == 0 {
            return 0;
        }

        let mut scanned = 0usize;
        let scan_budget = scan_budget.max(1).min(self.rx.len());

        while scanned < scan_budget {
            let src = self.scan_cursor % self.rx.len();
            self.scan_cursor = (self.scan_cursor + 1) % self.rx.len().max(1);
            scanned += 1;

            if src == self.shard_id || self.ready_enqueued[src] {
                continue;
            }
            let Some(lane) = self.rx[src].as_ref() else {
                continue;
            };
            if lane.consumer.is_empty() {
                continue;
            }
            self.ready_sources.push_back(src);
            self.ready_enqueued[src] = true;
        }

        let mut drained = 0usize;
        let per_source_budget = per_source_budget.max(1);

        while drained < max_total {
            let Some(src) = self.ready_sources.pop_front() else {
                break;
            };
            self.ready_enqueued[src] = false;

            let Some(lane) = self.rx[src].as_mut() else {
                continue;
            };

            let mut took = 0usize;
            while drained < max_total && took < per_source_budget {
                match lane.consumer.pop() {
                    Ok(item) => {
                        f(item);
                        drained += 1;
                        took += 1;
                    }
                    Err(PopError::Empty) => break,
                }
            }

            if !lane.consumer.is_empty() && !self.ready_enqueued[src] {
                self.ready_sources.push_back(src);
                self.ready_enqueued[src] = true;
            }
        }

        drained
    }
}

pub fn build_shard_mesh<T>(shard_count: usize, lane_capacity: usize) -> Vec<ShardMeshEndpoint<T>> {
    build_shard_mesh_with_spill(shard_count, lane_capacity, 0)
}

pub fn build_shard_mesh_with_spill<T>(
    shard_count: usize,
    lane_capacity: usize,
    spill_capacity: usize,
) -> Vec<ShardMeshEndpoint<T>> {
    if shard_count == 0 {
        return Vec::new();
    }

    let lane_capacity = lane_capacity.max(1);

    let mut tx_matrix: Vec<Vec<Option<TxLane<T>>>> = (0..shard_count)
        .map(|_| (0..shard_count).map(|_| None).collect())
        .collect();

    let mut rx_matrix: Vec<Vec<Option<RxLane<T>>>> = (0..shard_count)
        .map(|_| (0..shard_count).map(|_| None).collect())
        .collect();

    for src in 0..shard_count {
        for dst in 0..shard_count {
            if src == dst {
                continue;
            }
            let (producer, consumer) = RingBuffer::new(lane_capacity);
            tx_matrix[src][dst] = Some(TxLane {
                producer,
                spill: VecDeque::with_capacity(spill_capacity),
                spill_capacity,
            });
            rx_matrix[dst][src] = Some(RxLane { consumer });
        }
    }

    tx_matrix
        .into_iter()
        .zip(rx_matrix)
        .enumerate()
        .map(|(shard_id, (tx, rx))| ShardMeshEndpoint {
            shard_id,
            tx,
            rx,
            ready_sources: VecDeque::new(),
            ready_enqueued: vec![false; shard_count],
            scan_cursor: 0,
        })
        .collect()
}

pub(crate) fn normalize_placements(placements: &[SchedulerPlacement]) -> Vec<SchedulerPlacement> {
    if placements.is_empty() {
        return Vec::new();
    }

    let max_shard = placements
        .iter()
        .map(|placement| placement.shard)
        .max()
        .unwrap_or(0);

    let mut out: Vec<Option<SchedulerPlacement>> = vec![None; max_shard + 1];
    for &placement in placements {
        out[placement.shard] = Some(placement);
    }

    out.into_iter()
        .enumerate()
        .map(|(shard, placement)| {
            placement.unwrap_or(SchedulerPlacement {
                shard,
                core_id: shard,
                domain: 0,
            })
        })
        .collect()
}

pub(crate) fn build_route(shard: usize, placements: &[SchedulerPlacement]) -> Vec<usize> {
    if placements.is_empty() {
        return Vec::new();
    }

    let group = shard_group_from_placements(placements);
    group
        .push_path(shard)
        .map(|path| path.to_vec())
        .unwrap_or_default()
}

fn shard_group_from_placements(placements: &[SchedulerPlacement]) -> ShardGroup {
    let mut domains = HashSet::new();
    let topo_placements: Vec<TopologyShardPlacement> = placements
        .iter()
        .map(|placement| {
            domains.insert(placement.domain);
            TopologyShardPlacement {
                shard: placement.shard,
                core_id: placement.core_id,
                domain: placement.domain,
            }
        })
        .collect();

    let topology = ShardTopology {
        strategy: TopologyStrategy::GenericRoundRobin,
        domain_count: domains.len(),
        placements: topo_placements,
    };

    ShardGroup::from_topology(topology)
}
