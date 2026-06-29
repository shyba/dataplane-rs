use super::*;

pub(super) fn poll_start_net_descriptor_setup(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(
        region,
        0x80,
        USB_REQ_GET_DESCRIPTOR,
        GET_DEVICE_DESCRIPTOR,
        0,
        DEVICE_DESCRIPTOR_LEN as u16,
    );
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        0,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitNetDescriptorSetup;
}

pub(super) fn poll_wait_net_descriptor_setup(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartNetDescriptorIn;
    }
}

pub(super) fn poll_start_net_descriptor_in(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    clear_range(region, DESCRIPTOR_OFFSET, DEVICE_DESCRIPTOR_LEN);
    dma_clean_invalidate_range(region, DESCRIPTOR_OFFSET, DEVICE_DESCRIPTOR_LEN);
    start_control_transfer(
        0,
        dma_addr(region, DESCRIPTOR_OFFSET),
        DEVICE_DESCRIPTOR_LEN as u32,
        HCTSIZ_PID_DATA1,
        true,
    );
    task.state = UsbEthernetState::WaitNetDescriptorIn;
}

pub(super) fn poll_wait_net_descriptor_in(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        dma_invalidate_range(region, DESCRIPTOR_OFFSET, DEVICE_DESCRIPTOR_LEN);
        task.capture_descriptor(region);
        task.net_descriptor_valid =
            task.descriptor_valid && descriptor_class(region) == USB_CLASS_COMM;
        if task.net_descriptor_valid {
            task.state = UsbEthernetState::StartNetDescriptorStatus;
        } else {
            task.state = UsbEthernetState::Failed;
        }
    }
}

pub(super) fn poll_start_net_descriptor_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(0, dma_addr(region, SETUP_OFFSET), false);
    task.state = UsbEthernetState::WaitNetDescriptorStatus;
}

pub(super) fn poll_wait_net_descriptor_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartSetNetAddress;
    }
}

pub(super) fn poll_start_set_net_address(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(
        region,
        0x00,
        USB_REQ_SET_ADDRESS,
        USB_NET_ADDRESS as u16,
        0,
        0,
    );
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        0,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitSetNetAddress;
}

pub(super) fn poll_wait_set_net_address(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartSetNetAddressStatus;
    }
}

pub(super) fn poll_start_set_net_address_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(0, dma_addr(region, SETUP_OFFSET), true);
    task.state = UsbEthernetState::WaitSetNetAddressStatus;
}

pub(super) fn poll_wait_set_net_address_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartSetNetConfiguration;
    }
}

pub(super) fn poll_start_set_net_configuration(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(
        region,
        0x00,
        USB_REQ_SET_CONFIGURATION,
        RNDIS_CONFIGURATION_VALUE,
        0,
        0,
    );
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        USB_NET_ADDRESS,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitSetNetConfiguration;
}

pub(super) fn poll_wait_set_net_configuration(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartSetNetConfigurationStatus;
    }
}

pub(super) fn poll_start_set_net_configuration_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(USB_NET_ADDRESS, dma_addr(region, SETUP_OFFSET), true);
    task.state = UsbEthernetState::WaitSetNetConfigurationStatus;
}

pub(super) fn poll_wait_set_net_configuration_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.link_ready = true;
        task.rndis_configured = true;
        task.state = UsbEthernetState::StartRndisInitializeSetup;
    }
}
