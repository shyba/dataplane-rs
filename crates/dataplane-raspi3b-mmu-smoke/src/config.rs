pub(crate) const DATA_SHARD_COUNT: usize = 3;
pub(crate) const ETHERNET_REGION: usize = 3;
pub(crate) const REGION_COUNT: usize = 4;
pub(crate) const STEPS_PER_REGION: usize = 192;
pub(crate) const SHARD_REGION_SIZE: usize = 4096;
pub(crate) const FORBIDDEN_SHARD: usize = 1;

#[repr(align(4096))]
pub(crate) struct Page(pub(crate) [u64; 512]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShardId(pub(crate) usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProtectedShard {
    pub(crate) id: ShardId,
}
