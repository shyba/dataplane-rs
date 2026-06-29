use super::*;

pub(super) fn poll_start_rndis_set_filter_setup(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(
        region,
        0x21,
        USB_CDC_SEND_ENCAPSULATED_COMMAND,
        0,
        0,
        RNDIS_SET_MSG_LEN as u16,
    );
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        USB_NET_ADDRESS,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitRndisSetFilterSetup;
}

pub(super) fn poll_wait_rndis_set_filter_setup(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartRndisSetFilterData;
    }
}

pub(super) fn poll_start_rndis_set_filter_data(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_rndis_packet_filter(region);
    dma_clean_range(region, RNDIS_COMMAND_OFFSET, RNDIS_SET_MSG_LEN);
    start_control_transfer(
        USB_NET_ADDRESS,
        dma_addr(region, RNDIS_COMMAND_OFFSET),
        RNDIS_SET_MSG_LEN as u32,
        HCTSIZ_PID_DATA1,
        false,
    );
    task.state = UsbEthernetState::WaitRndisSetFilterData;
}

pub(super) fn poll_wait_rndis_set_filter_data(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartRndisSetFilterStatus;
    }
}

pub(super) fn poll_start_rndis_set_filter_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(USB_NET_ADDRESS, dma_addr(region, SETUP_OFFSET), true);
    task.state = UsbEthernetState::WaitRndisSetFilterStatus;
}

pub(super) fn poll_wait_rndis_set_filter_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartRndisSetFilterResponseSetup;
    }
}

pub(super) fn poll_start_rndis_set_filter_response_setup(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(
        region,
        0xa1,
        USB_CDC_GET_ENCAPSULATED_RESPONSE,
        0,
        0,
        RNDIS_SET_CMPLT_LEN as u16,
    );
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        USB_NET_ADDRESS,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitRndisSetFilterResponseSetup;
}

pub(super) fn poll_wait_rndis_set_filter_response_setup(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartRndisSetFilterResponseIn;
    }
}

pub(super) fn poll_start_rndis_set_filter_response_in(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    clear_range(region, RNDIS_RESPONSE_OFFSET, RNDIS_SET_CMPLT_LEN);
    dma_clean_invalidate_range(region, RNDIS_RESPONSE_OFFSET, RNDIS_SET_CMPLT_LEN);
    start_control_transfer(
        USB_NET_ADDRESS,
        dma_addr(region, RNDIS_RESPONSE_OFFSET),
        RNDIS_SET_CMPLT_LEN as u32,
        HCTSIZ_PID_DATA1,
        true,
    );
    task.state = UsbEthernetState::WaitRndisSetFilterResponseIn;
}

pub(super) fn poll_wait_rndis_set_filter_response_in(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        dma_invalidate_range(region, RNDIS_RESPONSE_OFFSET, RNDIS_SET_CMPLT_LEN);
        if task.capture_rndis_response(region, RNDIS_SET_CMPLT) {
            task.rndis_data_ready = true;
            task.state = UsbEthernetState::StartRndisSetFilterResponseStatus;
        } else {
            task.state = UsbEthernetState::Failed;
        }
    }
}

pub(super) fn poll_start_rndis_set_filter_response_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(USB_NET_ADDRESS, dma_addr(region, SETUP_OFFSET), false);
    task.state = UsbEthernetState::WaitRndisSetFilterResponseStatus;
}

pub(super) fn poll_wait_rndis_set_filter_response_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartBulkOut;
    }
}
