use crate::config::SHARD_REGION_SIZE;

use super::constants::*;
use super::usb::{cache_maint_range, CacheMaint};

pub(super) fn clear_range(region: &mut [u8; SHARD_REGION_SIZE], offset: usize, len: usize) {
    let mut index = 0usize;
    while index < len {
        write_u8(region, offset + index, 0);
        index += 1;
    }
}

pub(super) fn write_u32(region: &mut [u8; SHARD_REGION_SIZE], offset: usize, value: u32) {
    let bytes = value.to_le_bytes();
    write_u8(region, offset, bytes[0]);
    write_u8(region, offset + 1, bytes[1]);
    write_u8(region, offset + 2, bytes[2]);
    write_u8(region, offset + 3, bytes[3]);
}

pub(super) fn write_u16(region: &mut [u8; SHARD_REGION_SIZE], offset: usize, value: u16) {
    let bytes = value.to_le_bytes();
    write_u8(region, offset, bytes[0]);
    write_u8(region, offset + 1, bytes[1]);
}

pub(super) fn write_u8(region: &mut [u8; SHARD_REGION_SIZE], offset: usize, value: u8) {
    unsafe {
        core::ptr::write_volatile(region.as_mut_ptr().add(offset), value);
    }
}

pub(super) fn read_u32(region: &[u8; SHARD_REGION_SIZE], offset: usize) -> u32 {
    u32::from_le_bytes([
        read_u8(region, offset),
        read_u8(region, offset + 1),
        read_u8(region, offset + 2),
        read_u8(region, offset + 3),
    ])
}

pub(super) fn read_u8(region: &[u8; SHARD_REGION_SIZE], offset: usize) -> u8 {
    unsafe { core::ptr::read_volatile(region.as_ptr().add(offset)) }
}

pub(super) fn descriptor_class(region: &[u8; SHARD_REGION_SIZE]) -> u8 {
    read_u8(region, DESCRIPTOR_OFFSET + 4)
}

pub(super) fn dma_clean_range(region: &[u8; SHARD_REGION_SIZE], offset: usize, len: usize) {
    cache_maint_range(region, offset, len, CacheMaint::Clean);
}

pub(super) fn dma_clean_invalidate_range(
    region: &[u8; SHARD_REGION_SIZE],
    offset: usize,
    len: usize,
) {
    cache_maint_range(region, offset, len, CacheMaint::CleanInvalidate);
}

pub(super) fn dma_invalidate_range(region: &[u8; SHARD_REGION_SIZE], offset: usize, len: usize) {
    cache_maint_range(region, offset, len, CacheMaint::Invalidate);
}

pub(super) fn write_bytes(region: &mut [u8; SHARD_REGION_SIZE], offset: usize, bytes: &[u8]) {
    let mut index = 0usize;
    while index < bytes.len() {
        write_u8(region, offset + index, bytes[index]);
        index += 1;
    }
}
