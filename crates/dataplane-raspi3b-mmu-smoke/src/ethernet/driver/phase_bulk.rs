use super::*;

pub(super) fn poll_start_bulk_out(
    _task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
}

pub(super) fn poll_wait_bulk_out(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.frame_sent = true;
        task.state = UsbEthernetState::StartBulkIn;
    }
}

pub(super) fn poll_start_bulk_in(task: &mut UsbEthernetTask, region: &mut [u8; SHARD_REGION_SIZE]) {
    clear_range(region, BULK_RX_FRAME_OFFSET, RNDIS_MAX_TOTAL_SIZE as usize);
    dma_clean_invalidate_range(region, BULK_RX_FRAME_OFFSET, RNDIS_MAX_TOTAL_SIZE as usize);
    task.bulk_in_attempted = true;
    start_bulk_in_transfer(
        USB_NET_ADDRESS,
        dma_addr(region, BULK_RX_FRAME_OFFSET),
        RNDIS_MAX_TOTAL_SIZE,
    );
    task.state = UsbEthernetState::WaitBulkIn;
}

pub(super) fn poll_wait_bulk_in(task: &mut UsbEthernetTask, region: &mut [u8; SHARD_REGION_SIZE]) {
    if task.transfer_complete() {
        dma_invalidate_range(region, BULK_RX_FRAME_OFFSET, RNDIS_MAX_TOTAL_SIZE as usize);
        if task.capture_rx_frame(region) {
            task.frame_received = true;
            task.state = UsbEthernetState::PacketReceived;
        } else {
            task.state = UsbEthernetState::Failed;
        }
    }
}

pub(super) fn poll_terminal(task: &mut UsbEthernetTask, _region: &mut [u8; SHARD_REGION_SIZE]) {
    task.hprt = read_reg(HPRT);
}
