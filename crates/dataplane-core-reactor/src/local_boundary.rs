struct ShardBatch<T> {
    staged: Vec<T>,
    weight: usize,
}

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
                    weight: 0,
                })
                .collect(),
            weight_budget: weight_budget.max(1),
        }
    }

    pub fn push<F>(&mut self, shard: usize, item: T, publish: F)
    where
        F: FnMut(usize, &mut Vec<T>),
    {
        let should_flush = {
            let shard_batch = &mut self.shards[shard];
            shard_batch.staged.push(item);
            shard_batch.weight += 1;
            shard_batch.weight >= self.weight_budget
        };
        if should_flush {
            let mut publish = publish;
            self.flush_shard(shard, &mut publish);
        }
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
            shard_batch.weight += weight;
            shard_batch.weight >= self.weight_budget
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
        shard_batch.weight = 0;
        if is_empty {
            return;
        }
        publish(shard, &mut shard_batch.staged);
        shard_batch.staged.clear();
    }
}
