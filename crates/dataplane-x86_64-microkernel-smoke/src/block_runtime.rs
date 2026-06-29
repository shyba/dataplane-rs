use core::mem::size_of;

use crate::layout::{BLOCK_REQUEST_OFFSET, BLOCK_SECTOR_BYTES, BLOCK_TASK_BYTES, FS_SECTOR_OFFSET};
use crate::task_memory::{write_u32_region, write_u64_region, zero_region};
use crate::virtio_block::{VirtQueue, VirtioBlkReqHeader};
use crate::{arch, block_region_is_mmu_covered, fs_region_is_mmu_covered};

#[allow(dead_code)]
pub(crate) struct VirtqDesc {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16,
}

pub(crate) const BLOCK_DATA_OFFSET: usize = BLOCK_REQUEST_OFFSET + size_of::<VirtioBlkReqHeader>();
pub(crate) const BLOCK_STATUS_OFFSET: usize = BLOCK_DATA_OFFSET + BLOCK_SECTOR_BYTES;

pub(crate) fn prepare_block_read(
    region: &mut [u8; BLOCK_TASK_BYTES],
    queue: VirtQueue,
    sector: u64,
) -> Result<(), &'static str> {
    zero_region(
        region,
        BLOCK_REQUEST_OFFSET,
        size_of::<VirtioBlkReqHeader>() + BLOCK_SECTOR_BYTES + 1,
    );
    write_u32_region(region, BLOCK_REQUEST_OFFSET, VIRTIO_BLK_T_IN);
    write_u32_region(region, BLOCK_REQUEST_OFFSET + 4, 0);
    write_u64_region(region, BLOCK_REQUEST_OFFSET + 8, sector);
    region[BLOCK_STATUS_OFFSET] = 0xff;
    queue.submit_block_read(region)
}

#[cfg(feature = "fat32-write-proof")]
pub(crate) fn prepare_block_write(
    region: &mut [u8; BLOCK_TASK_BYTES],
    queue: VirtQueue,
    sector: u64,
) -> Result<(), &'static str> {
    zero_region(
        region,
        BLOCK_REQUEST_OFFSET,
        size_of::<VirtioBlkReqHeader>(),
    );
    write_u32_region(region, BLOCK_REQUEST_OFFSET, VIRTIO_BLK_T_OUT);
    write_u32_region(region, BLOCK_REQUEST_OFFSET + 4, 0);
    write_u64_region(region, BLOCK_REQUEST_OFFSET + 8, sector);
    region[BLOCK_STATUS_OFFSET] = 0xff;
    queue.submit_block_write(region)
}

pub(crate) fn copy_block_sector_to_fs() -> Result<(), &'static str> {
    if !block_region_is_mmu_covered() || !fs_region_is_mmu_covered() {
        return Err("sector-copy-region");
    }
    arch::with_block_and_fs_regions(|block, fs| {
        let mut index = 0;
        while index < BLOCK_SECTOR_BYTES {
            fs[FS_SECTOR_OFFSET + index] = block[BLOCK_DATA_OFFSET + index];
            index += 1;
        }
    });
    Ok(())
}

#[cfg(feature = "fat32-write-proof")]
pub(crate) fn copy_fs_sector_to_block() -> Result<(), &'static str> {
    if !block_region_is_mmu_covered() || !fs_region_is_mmu_covered() {
        return Err("sector-copy-region");
    }
    arch::with_block_and_fs_regions(|block, fs| {
        let mut index = 0;
        while index < BLOCK_SECTOR_BYTES {
            block[BLOCK_DATA_OFFSET + index] = fs[FS_SECTOR_OFFSET + index];
            index += 1;
        }
    });
    Ok(())
}

const VIRTIO_BLK_T_IN: u32 = 0;
#[cfg(feature = "fat32-write-proof")]
const VIRTIO_BLK_T_OUT: u32 = 1;
