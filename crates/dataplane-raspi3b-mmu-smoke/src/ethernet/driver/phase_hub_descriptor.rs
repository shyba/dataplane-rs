use super::*;

pub(super) fn poll_start_descriptor_setup(
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
    task.state = UsbEthernetState::WaitDescriptorSetup;
}

pub(super) fn poll_wait_descriptor_setup(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartDescriptorIn;
    }
}

pub(super) fn poll_start_descriptor_in(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    dma_clean_invalidate_range(region, DESCRIPTOR_OFFSET, DEVICE_DESCRIPTOR_LEN);
    start_control_transfer(
        0,
        dma_addr(region, DESCRIPTOR_OFFSET),
        DEVICE_DESCRIPTOR_LEN as u32,
        HCTSIZ_PID_DATA1,
        true,
    );
    task.state = UsbEthernetState::WaitDescriptorIn;
}

pub(super) fn poll_wait_descriptor_in(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        dma_invalidate_range(region, DESCRIPTOR_OFFSET, DEVICE_DESCRIPTOR_LEN);
        task.capture_descriptor(region);
        task.hub_descriptor_valid =
            task.descriptor_valid && descriptor_class(region) == USB_CLASS_HUB;
        if task.hub_descriptor_valid {
            task.state = UsbEthernetState::StartDescriptorStatus;
        } else {
            task.state = UsbEthernetState::Failed;
        }
    }
}

pub(super) fn poll_start_descriptor_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(0, dma_addr(region, SETUP_OFFSET), false);
    task.state = UsbEthernetState::WaitDescriptorStatus;
}

pub(super) fn poll_wait_descriptor_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartSetAddress;
    }
}

pub(super) fn poll_start_set_address(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(region, 0x00, USB_REQ_SET_ADDRESS, HUB_ADDRESS as u16, 0, 0);
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        0,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitSetAddress;
}

pub(super) fn poll_wait_set_address(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartSetAddressStatus;
    }
}

pub(super) fn poll_start_set_address_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(0, dma_addr(region, SETUP_OFFSET), true);
    task.state = UsbEthernetState::WaitSetAddressStatus;
}

pub(super) fn poll_wait_set_address_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartSetConfiguration;
    }
}

pub(super) fn poll_start_set_configuration(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(
        region,
        0x00,
        USB_REQ_SET_CONFIGURATION,
        HUB_CONFIGURATION_VALUE,
        0,
        0,
    );
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        HUB_ADDRESS,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitSetConfiguration;
}

pub(super) fn poll_wait_set_configuration(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartSetConfigurationStatus;
    }
}

pub(super) fn poll_start_set_configuration_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(HUB_ADDRESS, dma_addr(region, SETUP_OFFSET), true);
    task.state = UsbEthernetState::WaitSetConfigurationStatus;
}

pub(super) fn poll_wait_set_configuration_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.hub_configured = true;
        task.state = UsbEthernetState::StartHubPortPower;
    }
}

pub(super) fn poll_start_hub_port_power(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(
        region,
        0x23,
        USB_REQ_SET_FEATURE,
        HUB_PORT_POWER,
        HUB_PORT_ONE,
        0,
    );
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        HUB_ADDRESS,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitHubPortPower;
}

pub(super) fn poll_wait_hub_port_power(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartHubPortPowerStatus;
    }
}

pub(super) fn poll_start_hub_port_power_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(HUB_ADDRESS, dma_addr(region, SETUP_OFFSET), true);
    task.state = UsbEthernetState::WaitHubPortPowerStatus;
}

pub(super) fn poll_wait_hub_port_power_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.hub_port_powered = true;
        task.state = UsbEthernetState::StartHubPortReset;
    }
}

pub(super) fn poll_start_hub_port_reset(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(
        region,
        0x23,
        USB_REQ_SET_FEATURE,
        HUB_PORT_RESET,
        HUB_PORT_ONE,
        0,
    );
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        HUB_ADDRESS,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitHubPortReset;
}

pub(super) fn poll_wait_hub_port_reset(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartHubPortResetStatus;
    }
}

pub(super) fn poll_start_hub_port_reset_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(HUB_ADDRESS, dma_addr(region, SETUP_OFFSET), true);
    task.state = UsbEthernetState::WaitHubPortResetStatus;
}

pub(super) fn poll_wait_hub_port_reset_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.hub_port_reset = true;
        task.state = UsbEthernetState::StartHubPortStatusSetup;
    }
}

pub(super) fn poll_start_hub_port_status_setup(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    prepare_setup(region, 0xa3, USB_REQ_GET_STATUS, 0, HUB_PORT_ONE, 4);
    dma_clean_range(region, SETUP_OFFSET, 8);
    start_control_transfer(
        HUB_ADDRESS,
        dma_addr(region, SETUP_OFFSET),
        8,
        HCTSIZ_PID_SETUP,
        false,
    );
    task.state = UsbEthernetState::WaitHubPortStatusSetup;
}

pub(super) fn poll_wait_hub_port_status_setup(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartHubPortStatusIn;
    }
}

pub(super) fn poll_start_hub_port_status_in(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    clear_range(region, PORT_STATUS_OFFSET, 4);
    dma_clean_invalidate_range(region, PORT_STATUS_OFFSET, 4);
    start_control_transfer(
        HUB_ADDRESS,
        dma_addr(region, PORT_STATUS_OFFSET),
        4,
        HCTSIZ_PID_DATA1,
        true,
    );
    task.state = UsbEthernetState::WaitHubPortStatusIn;
}

pub(super) fn poll_wait_hub_port_status_in(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        dma_invalidate_range(region, PORT_STATUS_OFFSET, 4);
        task.port_status = read_u32(region, PORT_STATUS_OFFSET);
        if task.port_status & (HUB_PORT_STATUS_CONNECTION | HUB_PORT_STATUS_ENABLE)
            == (HUB_PORT_STATUS_CONNECTION | HUB_PORT_STATUS_ENABLE)
        {
            task.state = UsbEthernetState::StartHubPortStatusStatus;
        } else {
            task.state = UsbEthernetState::Failed;
        }
    }
}

pub(super) fn poll_start_hub_port_status_status(
    task: &mut UsbEthernetTask,
    region: &mut [u8; SHARD_REGION_SIZE],
) {
    start_control_status(HUB_ADDRESS, dma_addr(region, SETUP_OFFSET), false);
    task.state = UsbEthernetState::WaitHubPortStatusStatus;
}

pub(super) fn poll_wait_hub_port_status_status(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    if task.transfer_complete() {
        task.state = UsbEthernetState::StartNetDescriptorSetup;
    }
}
