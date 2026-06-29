use super::*;

pub(super) fn poll_start_rndis_initialize_setup(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(
        region,
        0x21,
        USB_CDC_SEND_ENCAPSULATED_COMMAND,
        0,
        0,
        RNDIS_INIT_MSG_LEN as u16,
    );
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        USB_NET_ADDRESS,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitRndisInitializeSetup;
}

pub(super) fn poll_wait_rndis_initialize_setup(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartRndisInitializeData;
    }
}

pub(super) fn poll_start_rndis_initialize_data(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_rndis_initialize(region);
    dma_clean_range(region, RNDIS_COMMAND_OFFSET, RNDIS_INIT_MSG_LEN);
    start_control_transfer(
        USB_NET_ADDRESS,
        dma_addr(region, RNDIS_COMMAND_OFFSET),
        RNDIS_INIT_MSG_LEN as u32,
        HCTSIZ_PID_DATA1,
        false,
    );
    task.state = UsbEthernetState::WaitRndisInitializeData;
}

pub(super) fn poll_wait_rndis_initialize_data(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartRndisInitializeStatus;
    }
}

pub(super) fn poll_start_rndis_initialize_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(USB_NET_ADDRESS, dma_addr(region, SETUP_OFFSET), true);
    task.state = UsbEthernetState::WaitRndisInitializeStatus;
}

pub(super) fn poll_wait_rndis_initialize_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartRndisInitializeResponseSetup;
    }
}

pub(super) fn poll_start_rndis_initialize_response_setup(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(
        region,
        0xa1,
        USB_CDC_GET_ENCAPSULATED_RESPONSE,
        0,
        0,
        RNDIS_INIT_CMPLT_LEN as u16,
    );
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        USB_NET_ADDRESS,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitRndisInitializeResponseSetup;
}

pub(super) fn poll_wait_rndis_initialize_response_setup(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartRndisInitializeResponseIn;
    }
}

pub(super) fn poll_start_rndis_initialize_response_in(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    clear_range(region, RNDIS_RESPONSE_OFFSET, RNDIS_INIT_CMPLT_LEN);
    dma_clean_invalidate_range(region, RNDIS_RESPONSE_OFFSET, RNDIS_INIT_CMPLT_LEN);
    start_control_transfer(
        USB_NET_ADDRESS,
        dma_addr(region, RNDIS_RESPONSE_OFFSET),
        RNDIS_INIT_CMPLT_LEN as u32,
        HCTSIZ_PID_DATA1,
        true,
    );
    task.state = UsbEthernetState::WaitRndisInitializeResponseIn;
}

pub(super) fn poll_wait_rndis_initialize_response_in(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        dma_invalidate_range(region, RNDIS_RESPONSE_OFFSET, RNDIS_INIT_CMPLT_LEN);
        if task.capture_rndis_response(region, RNDIS_INITIALIZE_CMPLT) {
            task.rndis_initialized = true;
            task.state = UsbEthernetState::StartRndisInitializeResponseStatus;
        } else {
            task.state = UsbEthernetState::Failed;
        }
    }
}

pub(super) fn poll_start_rndis_initialize_response_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(USB_NET_ADDRESS, dma_addr(region, SETUP_OFFSET), false);
    task.state = UsbEthernetState::WaitRndisInitializeResponseStatus;
}

pub(super) fn poll_wait_rndis_initialize_response_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartRndisSetFilterSetup;
    }
}
