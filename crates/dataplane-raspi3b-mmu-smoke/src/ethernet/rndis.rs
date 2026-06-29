use crate::config::SHARD_REGION_SIZE;
use dataplane_microkernel_core::EthernetFrameSpec;

use super::buffers::*;
use super::constants::*;

pub(super) fn prepare_rndis_initialize(region: &mut [u8; SHARD_REGION_SIZE]) {
    clear_range(region, RNDIS_COMMAND_OFFSET, RNDIS_INIT_MSG_LEN);
    write_u32(region, RNDIS_COMMAND_OFFSET, RNDIS_INITIALIZE_MSG);
    write_u32(region, RNDIS_COMMAND_OFFSET + 4, RNDIS_INIT_MSG_LEN as u32);
    write_u32(region, RNDIS_COMMAND_OFFSET + 8, 1);
    write_u32(region, RNDIS_COMMAND_OFFSET + 12, 1);
    write_u32(region, RNDIS_COMMAND_OFFSET + 16, 0);
    write_u32(region, RNDIS_COMMAND_OFFSET + 20, RNDIS_MAX_TOTAL_SIZE);
}

pub(super) fn prepare_rndis_packet_filter(region: &mut [u8; SHARD_REGION_SIZE]) {
    clear_range(region, RNDIS_COMMAND_OFFSET, RNDIS_SET_MSG_LEN);
    write_u32(region, RNDIS_COMMAND_OFFSET, RNDIS_SET_MSG);
    write_u32(region, RNDIS_COMMAND_OFFSET + 4, RNDIS_SET_MSG_LEN as u32);
    write_u32(region, RNDIS_COMMAND_OFFSET + 8, 2);
    write_u32(
        region,
        RNDIS_COMMAND_OFFSET + 12,
        OID_GEN_CURRENT_PACKET_FILTER,
    );
    write_u32(region, RNDIS_COMMAND_OFFSET + 16, 4);
    write_u32(region, RNDIS_COMMAND_OFFSET + 20, 20);
    write_u32(region, RNDIS_COMMAND_OFFSET + 24, RNDIS_PACKET_FILTER_ALL);
}

pub fn rndis_probe_frame() -> EthernetFrameSpec<'static> {
    EthernetFrameSpec {
        dst: [0xff; 6],
        src: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
        ethertype: 0x88b5,
        payload: &[],
        pad: 0,
    }
}

pub(super) fn prepare_rndis_probe_frame(
    region: &mut [u8; SHARD_REGION_SIZE],
    frame: EthernetFrameSpec<'_>,
) {
    clear_range(region, BULK_FRAME_OFFSET, RNDIS_PACKET_LEN);
    write_u32(region, BULK_FRAME_OFFSET, RNDIS_PACKET_MSG);
    write_u32(region, BULK_FRAME_OFFSET + 4, RNDIS_PACKET_LEN as u32);
    write_u32(region, BULK_FRAME_OFFSET + 8, (RNDIS_HEADER_LEN - 8) as u32);
    write_u32(region, BULK_FRAME_OFFSET + 12, ETHERNET_FRAME_LEN as u32);
    let frame_offset = BULK_FRAME_OFFSET + RNDIS_HEADER_LEN;
    write_bytes(region, frame_offset, &frame.dst);
    write_bytes(region, frame_offset + 6, &frame.src);
    write_u8(region, frame_offset + 12, (frame.ethertype >> 8) as u8);
    write_u8(region, frame_offset + 13, frame.ethertype as u8);
}
