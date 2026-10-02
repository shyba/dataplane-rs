use core::ptr::{read_volatile, write_volatile};

use crate::layout::{
    BLOCK_TASK_BYTES, NET_FRAME_OFFSET, NET_TASK_BYTES, RX_BUFFER_OFFSET, VIRTIO_NET_HDR_LEN,
};
use dataplane_microkernel_core::{Message, MessageBody};

pub(crate) struct BlockTaskMemory<'a> {
    pub(crate) bytes: &'a mut [u8; BLOCK_TASK_BYTES],
}

impl BlockTaskMemory<'_> {
    pub(crate) fn as_mut_bytes(&mut self) -> &mut [u8; BLOCK_TASK_BYTES] {
        self.bytes
    }
}

pub(crate) struct NetTaskMemory<'a> {
    pub(crate) bytes: &'a mut [u8; NET_TASK_BYTES],
}

impl NetTaskMemory<'_> {
    pub(crate) fn as_mut_bytes(&mut self) -> &mut [u8; NET_TASK_BYTES] {
        self.bytes
    }

    pub(crate) fn put_frame(&mut self, frame: &[u8]) -> Result<(), &'static str> {
        if frame.len() > NET_TASK_BYTES - NET_FRAME_OFFSET {
            return Err("net-frame-capacity");
        }
        let mut index = 0;
        while index < frame.len() {
            self.bytes[NET_FRAME_OFFSET + index] = frame[index];
            index += 1;
        }
        while index < NET_TASK_BYTES - NET_FRAME_OFFSET {
            self.bytes[NET_FRAME_OFFSET + index] = 0;
            index += 1;
        }
        Ok(())
    }

    pub(crate) fn copy_virtio_rx_frame(&mut self, raw_len: u32) -> Result<(), &'static str> {
        let raw_len = raw_len as usize;
        if raw_len > 2048 - VIRTIO_NET_HDR_LEN || raw_len > NET_TASK_BYTES - NET_FRAME_OFFSET {
            return Err("net-rx-frame-capacity");
        }
        let src = RX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN;
        let mut index = 0;
        while index < raw_len {
            self.bytes[NET_FRAME_OFFSET + index] = self.bytes[src + index];
            index += 1;
        }
        while index < NET_TASK_BYTES - NET_FRAME_OFFSET {
            self.bytes[NET_FRAME_OFFSET + index] = 0;
            index += 1;
        }
        Ok(())
    }

    pub(crate) fn frame(&self, len: usize) -> Result<&[u8], &'static str> {
        if len > NET_TASK_BYTES - NET_FRAME_OFFSET {
            return Err("net-frame-len");
        }
        Ok(&self.bytes[NET_FRAME_OFFSET..NET_FRAME_OFFSET + len])
    }

    pub(crate) fn frame_from_net_message(&self, message: Message) -> Result<&[u8], &'static str> {
        let len = match message.body {
            MessageBody::Pair(len, 0) => len as usize,
            _ => return Err("net-frame-message-body"),
        };
        self.frame(len)
    }
}

pub(crate) fn zero_region(region: &mut [u8; BLOCK_TASK_BYTES], offset: usize, len: usize) {
    for byte in &mut region[offset..offset + len] {
        *byte = 0;
    }
}

pub(crate) fn zero_net_region(region: &mut [u8; NET_TASK_BYTES], offset: usize, len: usize) {
    for byte in &mut region[offset..offset + len] {
        *byte = 0;
    }
}

// Device-shared ring fields require single volatile word accesses, not byte
// accesses that can tear index updates. Queue setup checks page-aligned bases;
// virtio callers supply aligned, in-bounds field offsets. These are x86-only
// little-endian accesses; publication/polling fences remain at the call sites.
pub(crate) fn write_u16_region(region: &mut [u8; BLOCK_TASK_BYTES], offset: usize, value: u16) {
    unsafe { write_volatile(region.as_mut_ptr().add(offset).cast::<u16>(), value) };
}

pub(crate) fn write_u16_net_region(region: &mut [u8; NET_TASK_BYTES], offset: usize, value: u16) {
    unsafe { write_volatile(region.as_mut_ptr().add(offset).cast::<u16>(), value) };
}

pub(crate) fn read_u16_region(region: &[u8; BLOCK_TASK_BYTES], offset: usize) -> u16 {
    unsafe { read_volatile(region.as_ptr().add(offset).cast::<u16>()) }
}

pub(crate) fn read_u16_net_region(region: &[u8; NET_TASK_BYTES], offset: usize) -> u16 {
    unsafe { read_volatile(region.as_ptr().add(offset).cast::<u16>()) }
}

pub(crate) fn read_u32_net_region(region: &[u8; NET_TASK_BYTES], offset: usize) -> u32 {
    unsafe { read_volatile(region.as_ptr().add(offset).cast::<u32>()) }
}

pub(crate) fn write_u32_region(region: &mut [u8; BLOCK_TASK_BYTES], offset: usize, value: u32) {
    region[offset] = value as u8;
    region[offset + 1] = (value >> 8) as u8;
    region[offset + 2] = (value >> 16) as u8;
    region[offset + 3] = (value >> 24) as u8;
}

pub(crate) fn write_u64_region(region: &mut [u8; BLOCK_TASK_BYTES], offset: usize, value: u64) {
    region[offset] = value as u8;
    region[offset + 1] = (value >> 8) as u8;
    region[offset + 2] = (value >> 16) as u8;
    region[offset + 3] = (value >> 24) as u8;
    region[offset + 4] = (value >> 32) as u8;
    region[offset + 5] = (value >> 40) as u8;
    region[offset + 6] = (value >> 48) as u8;
    region[offset + 7] = (value >> 56) as u8;
}
