use dataplane_microkernel_core::{
    EthernetFrameSpec, FixedNetworkDriver, NetworkFrameDescriptor, NetworkFrameDirection,
    NetworkFrameType, ReceivedFrame,
};

pub(in crate::ethernet::driver) use super::buffers::{
    clear_range, descriptor_class, dma_clean_invalidate_range, dma_clean_range,
    dma_invalidate_range, read_u32, read_u8, write_u16, write_u32, write_u8,
};
pub(in crate::ethernet::driver) use super::constants::{
    BULK_FRAME_OFFSET, BULK_RX_FRAME_OFFSET, DESCRIPTOR_OFFSET, DEVICE_DESCRIPTOR_LEN,
    DEVICE_DESCRIPTOR_TYPE, ETHERNET_FRAME_LEN, GAHBCFG, GAHBCFG_DMA_EN, GAHBCFG_GLBL_INTR_EN,
    GET_DEVICE_DESCRIPTOR, GSNPSID, HCINT0, HCINT_BBLERR, HCINT_CHHLTD, HCINT_DATATGLERR,
    HCINT_STALL, HCINT_XACTERR, HCINT_XFERCOMP, HCTSIZ0, HCTSIZ_PID_DATA1, HCTSIZ_PID_SETUP, HPRT,
    HPRT_CONN, HPRT_ENABLE, HPRT_POWER, HPRT_RESET, HUB_ADDRESS, HUB_CONFIGURATION_VALUE,
    HUB_PORT_ONE, HUB_PORT_POWER, HUB_PORT_RESET, HUB_PORT_STATUS_CONNECTION,
    HUB_PORT_STATUS_ENABLE, NET_RX_BUFFER_ID, NET_TX_BUFFER_ID, PORT_STATUS_OFFSET, REGION_MAGIC,
    RNDIS_COMMAND_OFFSET, RNDIS_CONFIGURATION_VALUE, RNDIS_HEADER_LEN, RNDIS_INITIALIZE_CMPLT,
    RNDIS_INIT_CMPLT_LEN, RNDIS_INIT_MSG_LEN, RNDIS_MAX_TOTAL_SIZE, RNDIS_PACKET_LEN,
    RNDIS_PACKET_MSG, RNDIS_RESPONSE_OFFSET, RNDIS_SET_CMPLT, RNDIS_SET_CMPLT_LEN,
    RNDIS_SET_MSG_LEN, RNDIS_STATUS_SUCCESS, SETUP_OFFSET, USB_CDC_GET_ENCAPSULATED_RESPONSE,
    USB_CDC_SEND_ENCAPSULATED_COMMAND, USB_CLASS_COMM, USB_CLASS_HUB, USB_NET_ADDRESS,
    USB_REQ_GET_DESCRIPTOR, USB_REQ_GET_STATUS, USB_REQ_SET_ADDRESS, USB_REQ_SET_CONFIGURATION,
    USB_REQ_SET_FEATURE,
};
pub(in crate::ethernet::driver) use super::rndis::{
    prepare_rndis_initialize, prepare_rndis_packet_filter, prepare_rndis_probe_frame,
};
pub(in crate::ethernet::driver) use super::state::UsbEthernetState;
pub(in crate::ethernet::driver) use super::usb::{
    dma_addr, hprt_write_value, read_reg, start_bulk_in_transfer, start_bulk_out_transfer,
    start_control_status, start_control_transfer, write_reg,
};
pub(in crate::ethernet::driver) use crate::config::SHARD_REGION_SIZE;

mod phase_bulk;
mod phase_hub_descriptor;
mod phase_net_descriptor;
mod phase_rndis_filter;
mod phase_rndis_init;
mod phase_usb_init;

pub struct UsbEthernetTask {
    pub(super) state: UsbEthernetState,
    pub(super) core_id: u32,
    pub(super) hprt: u32,
    pub(super) hcint: u32,
    pub(super) error: u32,
    pub(super) descriptor_head: u32,
    pub(super) descriptor_tail: u32,
    pub(super) frame_head: u32,
    pub(super) frame_tail: u32,
    pub(super) rx_frame_head: u32,
    pub(super) rx_frame_tail: u32,
    pub(super) rx_frame_len: u32,
    pub(super) rx_transport_len: u32,
    pub(super) port_status: u32,
    pub(super) polls: u32,
    pub(super) descriptor_valid: bool,
    pub(super) hub_descriptor_valid: bool,
    pub(super) hub_configured: bool,
    pub(super) hub_port_powered: bool,
    pub(super) hub_port_reset: bool,
    pub(super) net_descriptor_valid: bool,
    pub(super) link_ready: bool,
    pub(super) rndis_configured: bool,
    pub(super) rndis_initialized: bool,
    pub(super) rndis_data_ready: bool,
    pub(super) rndis_response_type: u32,
    pub(super) rndis_response_status: u32,
    pub(super) bulk_attempted: bool,
    pub(super) bulk_in_attempted: bool,
    pub(super) frame_sent: bool,
    pub(super) frame_received: bool,
}

impl UsbEthernetTask {
    pub const fn new() -> Self {
        Self {
            state: UsbEthernetState::ProbeCore,
            core_id: 0,
            hprt: 0,
            hcint: 0,
            error: 0,
            descriptor_head: 0,
            descriptor_tail: 0,
            frame_head: 0,
            frame_tail: 0,
            rx_frame_head: 0,
            rx_frame_tail: 0,
            rx_frame_len: 0,
            rx_transport_len: 0,
            port_status: 0,
            polls: 0,
            descriptor_valid: false,
            hub_descriptor_valid: false,
            hub_configured: false,
            hub_port_powered: false,
            hub_port_reset: false,
            net_descriptor_valid: false,
            link_ready: false,
            rndis_configured: false,
            rndis_initialized: false,
            rndis_data_ready: false,
            rndis_response_type: 0,
            rndis_response_status: 0,
            bulk_attempted: false,
            bulk_in_attempted: false,
            frame_sent: false,
            frame_received: false,
        }
    }

    pub fn poll(&mut self, region: &mut [u8; SHARD_REGION_SIZE]) {
        self.polls = self.polls.wrapping_add(1);
        match self.state {
            UsbEthernetState::ProbeCore => {
                phase_usb_init::poll_probe_core(self, region);
            }
            UsbEthernetState::PowerPort => {
                phase_usb_init::poll_power_port(self, region);
            }
            UsbEthernetState::ReleaseReset => {
                phase_usb_init::poll_release_reset(self, region);
            }
            UsbEthernetState::WaitEnabled => {
                phase_usb_init::poll_wait_enabled(self, region);
            }
            UsbEthernetState::StartDescriptorSetup => {
                phase_hub_descriptor::poll_start_descriptor_setup(self, region);
            }
            UsbEthernetState::WaitDescriptorSetup => {
                phase_hub_descriptor::poll_wait_descriptor_setup(self, region);
            }
            UsbEthernetState::StartDescriptorIn => {
                phase_hub_descriptor::poll_start_descriptor_in(self, region);
            }
            UsbEthernetState::WaitDescriptorIn => {
                phase_hub_descriptor::poll_wait_descriptor_in(self, region);
            }
            UsbEthernetState::StartDescriptorStatus => {
                phase_hub_descriptor::poll_start_descriptor_status(self, region);
            }
            UsbEthernetState::WaitDescriptorStatus => {
                phase_hub_descriptor::poll_wait_descriptor_status(self, region);
            }
            UsbEthernetState::StartSetAddress => {
                phase_hub_descriptor::poll_start_set_address(self, region);
            }
            UsbEthernetState::WaitSetAddress => {
                phase_hub_descriptor::poll_wait_set_address(self, region);
            }
            UsbEthernetState::StartSetAddressStatus => {
                phase_hub_descriptor::poll_start_set_address_status(self, region);
            }
            UsbEthernetState::WaitSetAddressStatus => {
                phase_hub_descriptor::poll_wait_set_address_status(self, region);
            }
            UsbEthernetState::StartSetConfiguration => {
                phase_hub_descriptor::poll_start_set_configuration(self, region);
            }
            UsbEthernetState::WaitSetConfiguration => {
                phase_hub_descriptor::poll_wait_set_configuration(self, region);
            }
            UsbEthernetState::StartSetConfigurationStatus => {
                phase_hub_descriptor::poll_start_set_configuration_status(self, region);
            }
            UsbEthernetState::WaitSetConfigurationStatus => {
                phase_hub_descriptor::poll_wait_set_configuration_status(self, region);
            }
            UsbEthernetState::StartHubPortPower => {
                phase_hub_descriptor::poll_start_hub_port_power(self, region);
            }
            UsbEthernetState::WaitHubPortPower => {
                phase_hub_descriptor::poll_wait_hub_port_power(self, region);
            }
            UsbEthernetState::StartHubPortPowerStatus => {
                phase_hub_descriptor::poll_start_hub_port_power_status(self, region);
            }
            UsbEthernetState::WaitHubPortPowerStatus => {
                phase_hub_descriptor::poll_wait_hub_port_power_status(self, region);
            }
            UsbEthernetState::StartHubPortReset => {
                phase_hub_descriptor::poll_start_hub_port_reset(self, region);
            }
            UsbEthernetState::WaitHubPortReset => {
                phase_hub_descriptor::poll_wait_hub_port_reset(self, region);
            }
            UsbEthernetState::StartHubPortResetStatus => {
                phase_hub_descriptor::poll_start_hub_port_reset_status(self, region);
            }
            UsbEthernetState::WaitHubPortResetStatus => {
                phase_hub_descriptor::poll_wait_hub_port_reset_status(self, region);
            }
            UsbEthernetState::StartHubPortStatusSetup => {
                phase_hub_descriptor::poll_start_hub_port_status_setup(self, region);
            }
            UsbEthernetState::WaitHubPortStatusSetup => {
                phase_hub_descriptor::poll_wait_hub_port_status_setup(self, region);
            }
            UsbEthernetState::StartHubPortStatusIn => {
                phase_hub_descriptor::poll_start_hub_port_status_in(self, region);
            }
            UsbEthernetState::WaitHubPortStatusIn => {
                phase_hub_descriptor::poll_wait_hub_port_status_in(self, region);
            }
            UsbEthernetState::StartHubPortStatusStatus => {
                phase_hub_descriptor::poll_start_hub_port_status_status(self, region);
            }
            UsbEthernetState::WaitHubPortStatusStatus => {
                phase_hub_descriptor::poll_wait_hub_port_status_status(self, region);
            }
            UsbEthernetState::StartNetDescriptorSetup => {
                phase_net_descriptor::poll_start_net_descriptor_setup(self, region);
            }
            UsbEthernetState::WaitNetDescriptorSetup => {
                phase_net_descriptor::poll_wait_net_descriptor_setup(self, region);
            }
            UsbEthernetState::StartNetDescriptorIn => {
                phase_net_descriptor::poll_start_net_descriptor_in(self, region);
            }
            UsbEthernetState::WaitNetDescriptorIn => {
                phase_net_descriptor::poll_wait_net_descriptor_in(self, region);
            }
            UsbEthernetState::StartNetDescriptorStatus => {
                phase_net_descriptor::poll_start_net_descriptor_status(self, region);
            }
            UsbEthernetState::WaitNetDescriptorStatus => {
                phase_net_descriptor::poll_wait_net_descriptor_status(self, region);
            }
            UsbEthernetState::StartSetNetAddress => {
                phase_net_descriptor::poll_start_set_net_address(self, region);
            }
            UsbEthernetState::WaitSetNetAddress => {
                phase_net_descriptor::poll_wait_set_net_address(self, region);
            }
            UsbEthernetState::StartSetNetAddressStatus => {
                phase_net_descriptor::poll_start_set_net_address_status(self, region);
            }
            UsbEthernetState::WaitSetNetAddressStatus => {
                phase_net_descriptor::poll_wait_set_net_address_status(self, region);
            }
            UsbEthernetState::StartSetNetConfiguration => {
                phase_net_descriptor::poll_start_set_net_configuration(self, region);
            }
            UsbEthernetState::WaitSetNetConfiguration => {
                phase_net_descriptor::poll_wait_set_net_configuration(self, region);
            }
            UsbEthernetState::StartSetNetConfigurationStatus => {
                phase_net_descriptor::poll_start_set_net_configuration_status(self, region);
            }
            UsbEthernetState::WaitSetNetConfigurationStatus => {
                phase_net_descriptor::poll_wait_set_net_configuration_status(self, region);
            }
            UsbEthernetState::StartRndisInitializeSetup => {
                phase_rndis_init::poll_start_rndis_initialize_setup(self, region);
            }
            UsbEthernetState::WaitRndisInitializeSetup => {
                phase_rndis_init::poll_wait_rndis_initialize_setup(self, region);
            }
            UsbEthernetState::StartRndisInitializeData => {
                phase_rndis_init::poll_start_rndis_initialize_data(self, region);
            }
            UsbEthernetState::WaitRndisInitializeData => {
                phase_rndis_init::poll_wait_rndis_initialize_data(self, region);
            }
            UsbEthernetState::StartRndisInitializeStatus => {
                phase_rndis_init::poll_start_rndis_initialize_status(self, region);
            }
            UsbEthernetState::WaitRndisInitializeStatus => {
                phase_rndis_init::poll_wait_rndis_initialize_status(self, region);
            }
            UsbEthernetState::StartRndisInitializeResponseSetup => {
                phase_rndis_init::poll_start_rndis_initialize_response_setup(self, region);
            }
            UsbEthernetState::WaitRndisInitializeResponseSetup => {
                phase_rndis_init::poll_wait_rndis_initialize_response_setup(self, region);
            }
            UsbEthernetState::StartRndisInitializeResponseIn => {
                phase_rndis_init::poll_start_rndis_initialize_response_in(self, region);
            }
            UsbEthernetState::WaitRndisInitializeResponseIn => {
                phase_rndis_init::poll_wait_rndis_initialize_response_in(self, region);
            }
            UsbEthernetState::StartRndisInitializeResponseStatus => {
                phase_rndis_init::poll_start_rndis_initialize_response_status(self, region);
            }
            UsbEthernetState::WaitRndisInitializeResponseStatus => {
                phase_rndis_init::poll_wait_rndis_initialize_response_status(self, region);
            }
            UsbEthernetState::StartRndisSetFilterSetup => {
                phase_rndis_filter::poll_start_rndis_set_filter_setup(self, region);
            }
            UsbEthernetState::WaitRndisSetFilterSetup => {
                phase_rndis_filter::poll_wait_rndis_set_filter_setup(self, region);
            }
            UsbEthernetState::StartRndisSetFilterData => {
                phase_rndis_filter::poll_start_rndis_set_filter_data(self, region);
            }
            UsbEthernetState::WaitRndisSetFilterData => {
                phase_rndis_filter::poll_wait_rndis_set_filter_data(self, region);
            }
            UsbEthernetState::StartRndisSetFilterStatus => {
                phase_rndis_filter::poll_start_rndis_set_filter_status(self, region);
            }
            UsbEthernetState::WaitRndisSetFilterStatus => {
                phase_rndis_filter::poll_wait_rndis_set_filter_status(self, region);
            }
            UsbEthernetState::StartRndisSetFilterResponseSetup => {
                phase_rndis_filter::poll_start_rndis_set_filter_response_setup(self, region);
            }
            UsbEthernetState::WaitRndisSetFilterResponseSetup => {
                phase_rndis_filter::poll_wait_rndis_set_filter_response_setup(self, region);
            }
            UsbEthernetState::StartRndisSetFilterResponseIn => {
                phase_rndis_filter::poll_start_rndis_set_filter_response_in(self, region);
            }
            UsbEthernetState::WaitRndisSetFilterResponseIn => {
                phase_rndis_filter::poll_wait_rndis_set_filter_response_in(self, region);
            }
            UsbEthernetState::StartRndisSetFilterResponseStatus => {
                phase_rndis_filter::poll_start_rndis_set_filter_response_status(self, region);
            }
            UsbEthernetState::WaitRndisSetFilterResponseStatus => {
                phase_rndis_filter::poll_wait_rndis_set_filter_response_status(self, region);
            }
            UsbEthernetState::StartBulkOut => {
                phase_bulk::poll_start_bulk_out(self, region);
            }
            UsbEthernetState::WaitBulkOut => {
                phase_bulk::poll_wait_bulk_out(self, region);
            }
            UsbEthernetState::StartBulkIn => {
                phase_bulk::poll_start_bulk_in(self, region);
            }
            UsbEthernetState::WaitBulkIn => {
                phase_bulk::poll_wait_bulk_in(self, region);
            }
            UsbEthernetState::PacketReceived | UsbEthernetState::Failed => {
                phase_bulk::poll_terminal(self, region);
            }
        }
        record_state(region, self);
    }

    pub fn frame_sent(&self) -> bool {
        self.frame_sent
    }

    fn usb_ready(&self) -> bool {
        self.descriptor_valid
            && self.hub_descriptor_valid
            && self.hub_configured
            && self.hub_port_powered
            && self.hub_port_reset
            && self.net_descriptor_valid
            && self.link_ready
            && self.rndis_configured
            && self.rndis_initialized
            && self.rndis_data_ready
            && self.bulk_attempted
            && self.bulk_in_attempted
            && self.frame_sent
            && self.frame_received
            && self.core_id != 0
    }

    pub(in crate::ethernet::driver) fn transfer_complete(&mut self) -> bool {
        self.hcint = read_reg(HCINT0);
        let error = self.hcint & (HCINT_STALL | HCINT_XACTERR | HCINT_BBLERR | HCINT_DATATGLERR);
        if error != 0 {
            self.error = error;
            self.state = UsbEthernetState::Failed;
            write_reg(HCINT0, self.hcint);
            return false;
        }
        if self.hcint & (HCINT_XFERCOMP | HCINT_CHHLTD) == (HCINT_XFERCOMP | HCINT_CHHLTD) {
            write_reg(HCINT0, self.hcint);
            return true;
        }
        false
    }

    pub(in crate::ethernet::driver) fn capture_descriptor(
        &mut self,
        region: &[u8; SHARD_REGION_SIZE],
    ) {
        self.descriptor_head = read_u32(region, DESCRIPTOR_OFFSET);
        self.descriptor_tail = read_u32(region, DESCRIPTOR_OFFSET + 4);
        self.descriptor_valid = read_u8(region, DESCRIPTOR_OFFSET) == DEVICE_DESCRIPTOR_LEN as u8
            && read_u8(region, DESCRIPTOR_OFFSET + 1) == DEVICE_DESCRIPTOR_TYPE;
        if !self.descriptor_valid {
            self.error = self.descriptor_head;
        }
    }

    pub(in crate::ethernet::driver) fn capture_rx_frame(
        &mut self,
        region: &[u8; SHARD_REGION_SIZE],
    ) -> bool {
        self.rx_frame_head = read_u32(region, BULK_RX_FRAME_OFFSET);
        self.rx_transport_len = RNDIS_MAX_TOTAL_SIZE - (read_reg(HCTSIZ0) & 0x7ffff);
        if self.rx_transport_len < RNDIS_HEADER_LEN as u32 || self.rx_frame_head != RNDIS_PACKET_MSG
        {
            self.error = self.rx_frame_head;
            return false;
        }
        let packet_len = read_u32(region, BULK_RX_FRAME_OFFSET + 4);
        let data_offset = read_u32(region, BULK_RX_FRAME_OFFSET + 8);
        let data_len = read_u32(region, BULK_RX_FRAME_OFFSET + 12);
        let frame_offset = 8u32.saturating_add(data_offset);
        if packet_len > self.rx_transport_len
            || frame_offset < RNDIS_HEADER_LEN as u32
            || data_len < ETHERNET_FRAME_LEN as u32
            || frame_offset + data_len > packet_len
            || frame_offset.saturating_add(data_len) > RNDIS_MAX_TOTAL_SIZE
            || BULK_RX_FRAME_OFFSET.saturating_add(frame_offset.saturating_add(data_len) as usize)
                > SHARD_REGION_SIZE
        {
            self.error = packet_len;
            return false;
        }
        self.rx_frame_len = data_len;
        self.rx_frame_tail = read_u32(
            region,
            BULK_RX_FRAME_OFFSET + frame_offset as usize + data_len as usize - 4,
        );
        true
    }

    pub(in crate::ethernet::driver) fn capture_rndis_response(
        &mut self,
        region: &[u8; SHARD_REGION_SIZE],
        expected_type: u32,
    ) -> bool {
        self.rndis_response_type = read_u32(region, RNDIS_RESPONSE_OFFSET);
        self.rndis_response_status = read_u32(region, RNDIS_RESPONSE_OFFSET + 12);
        self.rndis_response_type == expected_type
            && self.rndis_response_status == RNDIS_STATUS_SUCCESS
    }

    fn capture_frame(&mut self, region: &[u8; SHARD_REGION_SIZE]) {
        self.frame_head = read_u32(region, BULK_FRAME_OFFSET);
        self.frame_tail = read_u32(region, BULK_FRAME_OFFSET + RNDIS_PACKET_LEN - 4);
    }
}

impl FixedNetworkDriver<[u8; SHARD_REGION_SIZE], ()> for UsbEthernetTask {
    fn init(&mut self, memory: &mut [u8; SHARD_REGION_SIZE]) -> Result<(), ()> {
        self.poll(memory);
        Ok(())
    }
    fn arm_receive(&mut self, _memory: &mut [u8; SHARD_REGION_SIZE]) -> Result<(), ()> {
        Ok(())
    }
    fn transmit_frame(
        &mut self,
        memory: &mut [u8; SHARD_REGION_SIZE],
        frame: EthernetFrameSpec<'_>,
    ) -> Result<(), ()> {
        if !matches!(self.state, UsbEthernetState::StartBulkOut) {
            return Ok(());
        }
        let _descriptor = NetworkFrameDescriptor::new(
            NetworkFrameDirection::Tx,
            NetworkFrameType::Ethernet,
            NET_TX_BUFFER_ID,
            frame.padded_len(ETHERNET_FRAME_LEN),
            ETHERNET_FRAME_LEN,
        )
        .map_err(|_| ())?;
        prepare_rndis_probe_frame(memory, frame);
        self.capture_frame(memory);
        dma_clean_range(memory, BULK_FRAME_OFFSET, RNDIS_PACKET_LEN);
        self.bulk_attempted = true;
        start_bulk_out_transfer(
            USB_NET_ADDRESS,
            dma_addr(memory, BULK_FRAME_OFFSET),
            RNDIS_PACKET_LEN as u32,
        );
        self.state = UsbEthernetState::WaitBulkOut;
        Ok(())
    }
    fn receive_frame(
        &mut self,
        _memory: &mut [u8; SHARD_REGION_SIZE],
    ) -> Result<ReceivedFrame, ()> {
        if !self.usb_ready() {
            return Err(());
        }
        let descriptor = NetworkFrameDescriptor::new(
            NetworkFrameDirection::Rx,
            NetworkFrameType::Ethernet,
            NET_RX_BUFFER_ID,
            self.rx_frame_len as usize,
            RNDIS_MAX_TOTAL_SIZE as usize,
        )
        .map_err(|_| ())?;
        Ok(ReceivedFrame {
            descriptor,
            transport_len: self.rx_transport_len,
        })
    }
}

pub(in crate::ethernet::driver) fn prepare_setup(
    region: &mut [u8; SHARD_REGION_SIZE],
    request_type: u8,
    request: u8,
    value: u16,
    index_value: u16,
    length: u16,
) {
    write_u8(region, SETUP_OFFSET, request_type);
    write_u8(region, SETUP_OFFSET + 1, request);
    write_u16(region, SETUP_OFFSET + 2, value);
    write_u16(region, SETUP_OFFSET + 4, index_value);
    write_u16(region, SETUP_OFFSET + 6, length);
    if request_type == 0x80 && request == 0x06 {
        let mut descriptor = 0usize;
        while descriptor < DEVICE_DESCRIPTOR_LEN {
            write_u8(region, DESCRIPTOR_OFFSET + descriptor, 0);
            descriptor += 1;
        }
    }
}

fn record_state(region: &mut [u8; SHARD_REGION_SIZE], task: &UsbEthernetTask) {
    write_u32(region, 0, REGION_MAGIC);
    write_u32(region, 4, task.polls);
    write_u32(region, 8, task.core_id);
    write_u32(region, 12, task.hprt);
    write_u32(region, 16, task.state as u32);
    write_u32(region, 20, task.hcint);
    write_u32(region, 24, task.descriptor_head);
    write_u32(region, 28, task.descriptor_tail);
    write_u32(region, 32, task.error);
    write_u32(region, 36, task.frame_head);
    write_u32(region, 40, task.frame_tail);
    write_u32(region, 44, task.bulk_attempted as u32);
    write_u32(region, 48, task.frame_sent as u32);
    write_u32(region, 52, task.rndis_configured as u32);
    write_u32(region, 56, task.rndis_initialized as u32);
    write_u32(region, 60, task.rndis_data_ready as u32);
    write_u32(region, 64, task.bulk_in_attempted as u32);
    write_u32(region, 68, task.frame_received as u32);
    write_u32(region, 80, task.port_status);
    write_u32(region, 84, task.hub_descriptor_valid as u32);
    write_u32(region, 88, task.hub_configured as u32);
    write_u32(region, 92, task.hub_port_powered as u32);
    write_u32(region, 96, task.hub_port_reset as u32);
    write_u32(region, 100, task.net_descriptor_valid as u32);
    write_u32(region, 104, task.rx_frame_head);
    write_u32(region, 108, task.rx_frame_tail);
    write_u32(region, 112, task.rx_frame_len);
    write_u32(region, 116, task.rx_transport_len);
    write_u32(region, 72, task.rndis_response_type);
    write_u32(region, 76, task.rndis_response_status);
}
