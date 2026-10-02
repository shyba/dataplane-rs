struct ShardBatch<T> {
    staged: Vec<T>,
    remaining_weight: usize,
}

/// Per-shard weighted batching with caller-owned publication.
/// Shard indices must be in range. The publication callback may drain the batch;
/// any items it leaves behind are dropped when the callback returns.
pub struct LocalShardPublisher<T> {
    shards: Vec<ShardBatch<T>>,
    weight_budget: usize,
}

impl<T> LocalShardPublisher<T> {
    pub fn new(shard_count: usize, weight_budget: usize) -> Self {
        Self {
            shards: (0..shard_count)
                .map(|_| ShardBatch {
                    staged: Vec::with_capacity(weight_budget),
                    remaining_weight: weight_budget.max(1),
                })
                .collect(),
            weight_budget: weight_budget.max(1),
        }
    }

    pub fn push<F>(&mut self, shard: usize, item: T, publish: F)
    where
        F: FnMut(usize, &mut Vec<T>),
    {
        self.push_known_weight(shard, item, 1, publish);
    }

    pub fn push_weighted<F>(&mut self, shard: usize, item: T, weight: usize, publish: F)
    where
        F: FnMut(usize, &mut Vec<T>),
    {
        let weight = if weight == 0 { 1 } else { weight };
        self.push_known_weight(shard, item, weight, publish);
    }

    pub fn push_known_weight<F>(&mut self, shard: usize, item: T, weight: usize, mut publish: F)
    where
        F: FnMut(usize, &mut Vec<T>),
    {
        debug_assert!(weight > 0);
        let should_flush = {
            let shard_batch = &mut self.shards[shard];
            shard_batch.staged.push(item);
            let should_flush = weight >= shard_batch.remaining_weight;
            // Oversized weights flush immediately, so this wrapped value is
            // reset before it can be observed by a callback or another push.
            shard_batch.remaining_weight = shard_batch.remaining_weight.wrapping_sub(weight);
            should_flush
        };
        if should_flush {
            self.flush_shard(shard, &mut publish);
        }
    }

    pub fn flush_all<F>(&mut self, mut publish: F)
    where
        F: FnMut(usize, &mut Vec<T>),
    {
        for shard in 0..self.shards.len() {
            self.flush_shard(shard, &mut publish);
        }
    }

    fn flush_shard<F>(&mut self, shard: usize, publish: &mut F)
    where
        F: FnMut(usize, &mut Vec<T>),
    {
        let shard_batch = &mut self.shards[shard];
        let is_empty = shard_batch.staged.is_empty();
        shard_batch.remaining_weight = self.weight_budget;
        if is_empty {
            return;
        }
        publish(shard, &mut shard_batch.staged);
        shard_batch.staged.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::LocalShardPublisher;

    fn collect_publishes(
        publisher: &mut LocalShardPublisher<u32>,
        pushes: &[(usize, u32)],
    ) -> Vec<(usize, Vec<u32>)> {
        let mut published = Vec::new();
        for &(shard, item) in pushes {
            publisher.push(shard, item, |shard, staged| {
                published.push((shard, staged.clone()));
            });
        }
        published
    }

    #[test]
    fn push_flushes_when_weight_budget_reached() {
        let mut publisher = LocalShardPublisher::new(2, 2);
        let published = collect_publishes(&mut publisher, &[(0, 1), (1, 10), (0, 2)]);
        assert_eq!(published, vec![(0, vec![1, 2])]);
    }

    #[test]
    fn flush_all_publishes_remaining_and_skips_empty_shards() {
        let mut publisher = LocalShardPublisher::new(3, 100);
        publisher.push(0, 1, |_, _| unreachable!("under budget"));
        publisher.push(2, 3, |_, _| unreachable!("under budget"));

        let mut published = Vec::new();
        publisher.flush_all(|shard, staged| published.push((shard, staged.clone())));
        assert_eq!(published, vec![(0, vec![1]), (2, vec![3])]);

        published.clear();
        publisher.flush_all(|shard, staged| published.push((shard, staged.clone())));
        assert!(published.is_empty());
    }

    #[test]
    fn overflowing_weight_flushes_instead_of_wrapping() {
        let mut publisher = LocalShardPublisher::new(1, 16);
        publisher.push_weighted(0, 1, 1, |_, _| panic!("under budget"));
        let mut published = Vec::new();
        publisher.push_weighted(0, 2, usize::MAX, |_, batch| published.extend_from_slice(batch));
        assert_eq!(published, vec![1, 2]);
    }

    #[test]
    fn weighted_push_reaches_budget_faster() {
        let mut publisher = LocalShardPublisher::new(1, 4);
        let mut published = Vec::new();
        publisher.push_weighted(0, 7, 0, |shard, staged| {
            published.push((shard, staged.clone()))
        });
        assert!(published.is_empty(), "zero weight is clamped to 1");
        publisher.push_weighted(0, 8, 3, |shard, staged| {
            published.push((shard, staged.clone()))
        });
        assert_eq!(published, vec![(0, vec![7, 8])]);
    }
}
