use core::arch::asm;

use crate::config::SHARD_REGION_SIZE;

use super::constants::*;

pub(super) fn read_reg(offset: usize) -> u32 {
    unsafe { core::ptr::read_volatile((USB_BASE + offset) as *const u32) }
}

pub(super) fn write_reg(offset: usize, value: u32) {
    unsafe { core::ptr::write_volatile((USB_BASE + offset) as *mut u32, value) }
}

pub(super) fn hprt_write_value(hprt: u32) -> u32 {
    hprt & !(HPRT_CONN_DETECT
        | HPRT_ENABLE
        | HPRT_ENABLE_CHANGE
        | HPRT_OVERCURRENT_CHANGE
        | HPRT_RESET)
}

pub(super) fn start_control_transfer(
    device_address: u32,
    buffer_dma: u32,
    len: u32,
    pid: u32,
    in_direction: bool,
) {
    start_transfer(
        device_address,
        EP0,
        USB_ENDPOINT_XFER_CONTROL,
        buffer_dma,
        len,
        pid,
        in_direction,
    );
}

pub(super) fn start_control_status(device_address: u32, buffer_dma: u32, in_direction: bool) {
    start_transfer(
        device_address,
        EP0,
        USB_ENDPOINT_XFER_CONTROL,
        buffer_dma,
        0,
        HCTSIZ_PID_DATA1,
        in_direction,
    );
}

pub(super) fn start_bulk_out_transfer(device_address: u32, buffer_dma: u32, len: u32) {
    start_transfer(
        device_address,
        EP_BULK_OUT,
        USB_ENDPOINT_XFER_BULK,
        buffer_dma,
        len,
        HCTSIZ_PID_DATA0,
        false,
    );
}

pub(super) fn start_bulk_in_transfer(device_address: u32, buffer_dma: u32, len: u32) {
    start_transfer(
        device_address,
        EP_BULK_IN,
        USB_ENDPOINT_XFER_BULK,
        buffer_dma,
        len,
        HCTSIZ_PID_DATA0,
        true,
    );
}

fn start_transfer(
    device_address: u32,
    endpoint: u32,
    endpoint_type: u32,
    buffer_dma: u32,
    len: u32,
    pid: u32,
    in_direction: bool,
) {
    write_reg(HCINT0, HCINT_ALL);
    write_reg(HCINTMSK0, HCINT_XFERCOMP | HCINT_CHHLTD);
    write_reg(HCDMA0, buffer_dma);
    write_reg(HCTSIZ0, hctsiz(len, 1, pid));
    unsafe { asm!("dsb sy", options(nostack, preserves_flags)) };
    write_reg(
        HCCHAR0,
        hcchar(device_address, endpoint, endpoint_type, in_direction, 64) | HCCHAR_CHENA,
    );
}

fn hcchar(
    device_address: u32,
    endpoint: u32,
    endpoint_type: u32,
    in_direction: bool,
    max_packet_size: u32,
) -> u32 {
    let direction = if in_direction { HCCHAR_EPDIR_IN } else { 0 };
    (max_packet_size & 0x7ff)
        | ((endpoint & 0xf) << HCCHAR_EPNUM_SHIFT)
        | ((endpoint_type & 0x3) << HCCHAR_EPTYPE_SHIFT)
        | ((device_address & 0x7f) << HCCHAR_DEVADDR_SHIFT)
        | direction
        | HCCHAR_MULTI_COUNT_1
}

fn hctsiz(len: u32, packet_count: u32, pid: u32) -> u32 {
    (len & 0x7ffff) | ((packet_count & 0x3ff) << 19) | ((pid & 0x3) << 29)
}

pub(super) fn dma_addr(region: &[u8; SHARD_REGION_SIZE], offset: usize) -> u32 {
    (region.as_ptr() as usize + offset) as u32
}

pub(super) fn cache_maint_range(
    region: &[u8; SHARD_REGION_SIZE],
    offset: usize,
    len: usize,
    op: CacheMaint,
) {
    const CACHE_LINE: usize = 64;
    let start = ((region.as_ptr() as usize) + offset) & !(CACHE_LINE - 1);
    let end = ((region.as_ptr() as usize) + offset + len + CACHE_LINE - 1) & !(CACHE_LINE - 1);
    let mut line = start;
    while line < end {
        unsafe {
            match op {
                CacheMaint::Clean => {
                    asm!("dc cvac, {}", in(reg) line, options(nostack, preserves_flags));
                }
                CacheMaint::CleanInvalidate => {
                    asm!("dc civac, {}", in(reg) line, options(nostack, preserves_flags));
                }
                CacheMaint::Invalidate => {
                    asm!("dc ivac, {}", in(reg) line, options(nostack, preserves_flags));
                }
            }
        }
        line += CACHE_LINE;
    }
    unsafe {
        asm!("dsb sy; isb", options(nostack, preserves_flags));
    }
}

pub(super) enum CacheMaint {
    Clean,
    CleanInvalidate,
    Invalidate,
}
