use core::arch::asm;
use core::mem::size_of;

use crate::{
    align_up, compiler_fence, ethernet_frame_len, inw, net_frame_message_body,
    net_region_is_mmu_covered, net_tx_probe_frame, outb, outl, outw, pci, protected_net_region,
    read_u16_net_region, read_u32_net_region, write_u16_net_region, zero_net_region,
    EthernetFrameSpec, NetTaskMemory, NetworkFrameDescriptor, NetworkFrameDirection,
    NetworkFrameType, NetworkReply, VirtqDesc, CAP_KERNEL, EP_TCPIP, ETHERNET_HEADER_BYTES,
    NET_FRAME_OFFSET, NET_QUEUE_CAP, NET_RX_BUFFER_ID, NET_TASK_BYTES, NET_TX_BUFFER_ID,
    NET_VIRTQ_BYTES, REQUEST_TCPIP_NET, RX_BUFFER_OFFSET, RX_QUEUE_OFFSET, TASK_NET, TASK_TCPIP,
    TX_BUFFER_OFFSET, TX_FRAME_LEN, TX_PACKET_LEN, TX_QUEUE_OFFSET, VIRTIO_NET_HDR_LEN,
    VIRTIO_PCI_GUEST_FEATURES, VIRTIO_PCI_QUEUE_NOTIFY, VIRTIO_PCI_QUEUE_NUM, VIRTIO_PCI_QUEUE_PFN,
    VIRTIO_PCI_QUEUE_SEL, VIRTIO_PCI_STATUS, VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DRIVER,
    VIRTIO_STATUS_DRIVER_OK, VIRTQ_DESC_F_WRITE,
};
use dataplane_microkernel_core::{EndpointId, Message, MessageBody, RequestId};

#[derive(Clone, Copy)]
pub(crate) struct NetVirtQueue {
    offset: usize,
    size: u16,
}

impl NetVirtQueue {
    pub(crate) const fn new(offset: usize, size: u16) -> Self {
        Self { offset, size }
    }

    pub(crate) fn submit_read_only(
        self,
        region: &mut [u8; NET_TASK_BYTES],
        buffer_offset: usize,
        len: usize,
    ) -> Result<(), &'static str> {
        let desc = VirtqDesc {
            addr: region[buffer_offset..].as_ptr() as u64,
            len: len as u32,
            flags: 0,
            next: 0,
        };
        self.write_desc(region, 0, desc);
        self.write_avail(region, 0);
        Ok(())
    }

    pub(crate) fn submit_writable(
        self,
        region: &mut [u8; NET_TASK_BYTES],
        buffer_offset: usize,
        len: usize,
    ) -> Result<(), &'static str> {
        let desc = VirtqDesc {
            addr: region[buffer_offset..].as_ptr() as u64,
            len: len as u32,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        };
        self.write_desc(region, 0, desc);
        self.write_avail(region, 0);
        Ok(())
    }

    fn write_desc(self, region: &mut [u8; NET_TASK_BYTES], index: usize, desc: VirtqDesc) {
        let ptr = region[self.offset..].as_mut_ptr() as *mut VirtqDesc;
        unsafe { core::ptr::write_volatile(ptr.add(index), desc) };
    }

    fn write_avail(self, region: &mut [u8; NET_TASK_BYTES], desc_index: u16) {
        let avail = self.offset + size_of::<VirtqDesc>() * self.size as usize;
        write_u16_net_region(region, avail, 0);
        let idx = read_u16_net_region(region, avail + 2);
        write_u16_net_region(
            region,
            avail + 4 + (idx as usize % self.size as usize) * 2,
            desc_index,
        );
        crate::compiler_fence();
        write_u16_net_region(region, avail + 2, idx.wrapping_add(1));
    }

    pub(crate) fn used_idx(self, region: &[u8; NET_TASK_BYTES]) -> u16 {
        let used = align_up(
            self.offset + size_of::<VirtqDesc>() * self.size as usize + 4 + self.size as usize * 2,
            4096,
        );
        read_u16_net_region(region, used + 2)
    }

    pub(crate) fn used_len(self, region: &[u8; NET_TASK_BYTES], previous_idx: u16) -> u32 {
        let used = align_up(
            self.offset + size_of::<VirtqDesc>() * self.size as usize + 4 + self.size as usize * 2,
            4096,
        );
        let entry = used + 4 + (previous_idx as usize % self.size as usize) * 8;
        read_u32_net_region(region, entry + 4)
    }
}

pub(crate) fn prepare_net_rx_buffer(
    region: &mut [u8; NET_TASK_BYTES],
    rx: NetVirtQueue,
) -> Result<(), &'static str> {
    zero_net_region(region, RX_BUFFER_OFFSET, 2048);
    rx.submit_writable(region, RX_BUFFER_OFFSET, 2048)
}

pub(crate) fn prepare_net_tx_frame(
    region: &mut [u8; NET_TASK_BYTES],
    frame_spec: EthernetFrameSpec<'_>,
) -> Result<(), &'static str> {
    if frame_spec.payload.len() > TX_FRAME_LEN - ETHERNET_HEADER_BYTES {
        return Err("virtio-net-tx-payload-len");
    }

    zero_net_region(region, TX_BUFFER_OFFSET, TX_PACKET_LEN);
    let frame = TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN;
    region[frame..frame + 6].copy_from_slice(&frame_spec.dst);
    region[frame + 6..frame + 12].copy_from_slice(&frame_spec.src);
    region[frame + 12] = (frame_spec.ethertype >> 8) as u8;
    region[frame + 13] = frame_spec.ethertype as u8;
    let payload = frame + ETHERNET_HEADER_BYTES;
    region[payload..payload + frame_spec.payload.len()].copy_from_slice(frame_spec.payload);
    for index in ETHERNET_HEADER_BYTES + frame_spec.payload.len()..TX_FRAME_LEN {
        region[frame + index] = frame_spec.pad;
    }
    Ok(())
}

pub(crate) struct VirtioLegacyPciNet {
    io_base: u16,
    rx: NetVirtQueue,
    tx: NetVirtQueue,
    rx_used_seen: u16,
    tx_used_seen: u16,
}

impl VirtioLegacyPciNet {
    pub(crate) const fn new() -> Self {
        Self {
            io_base: 0,
            rx: NetVirtQueue::new(RX_QUEUE_OFFSET, 0),
            tx: NetVirtQueue::new(TX_QUEUE_OFFSET, 0),
            rx_used_seen: 0,
            tx_used_seen: 0,
        }
    }

    pub(crate) fn init(&mut self, memory: &mut NetTaskMemory<'_>) -> Result<(), &'static str> {
        let region = memory.as_mut_bytes();
        self.find_device()?;
        self.begin_init();
        self.rx = self.setup_queue(region, 0, RX_QUEUE_OFFSET)?;
        self.tx = self.setup_queue(region, 1, TX_QUEUE_OFFSET)?;
        self.rx_used_seen = self.rx.used_idx(region);
        self.tx_used_seen = self.tx.used_idx(region);
        self.finish_init();
        self.arm_receive(memory)?;
        Ok(())
    }

    pub(crate) fn transmit_frame(
        &mut self,
        memory: &mut NetTaskMemory<'_>,
        frame: EthernetFrameSpec<'_>,
    ) -> Result<(), &'static str> {
        let frame_len = ethernet_frame_len(frame.payload.len());
        let _descriptor = NetworkFrameDescriptor::new(
            NetworkFrameDirection::Tx,
            NetworkFrameType::Ethernet,
            NET_TX_BUFFER_ID,
            frame_len,
            TX_FRAME_LEN,
        )
        .map_err(|_| "virtio-net-tx-descriptor")?;
        let region = memory.as_mut_bytes();
        prepare_net_tx_frame(region, frame)?;
        self.tx
            .submit_read_only(region, TX_BUFFER_OFFSET, TX_PACKET_LEN)?;
        compiler_fence();
        outw(self.io_base + VIRTIO_PCI_QUEUE_NOTIFY, 1);

        for _ in 0..100_000 {
            compiler_fence();
            let used = self.tx.used_idx(region);
            if used != self.tx_used_seen {
                self.tx_used_seen = used;
                return Ok(());
            }
            unsafe { asm!("pause", options(nomem, nostack, preserves_flags)) };
        }
        Err("virtio-net-tx-timeout")
    }

    pub(crate) fn poll_receive_frame(
        &mut self,
        memory: &mut NetTaskMemory<'_>,
    ) -> Result<Option<u32>, &'static str> {
        let region = memory.as_mut_bytes();
        for _ in 0..1000 {
            compiler_fence();
            let used = self.rx.used_idx(region);
            if used != self.rx_used_seen {
                let len = self.rx.used_len(region, self.rx_used_seen);
                self.rx_used_seen = used;
                if len as usize <= VIRTIO_NET_HDR_LEN || len as usize > 2048 {
                    return Err("virtio-net-rx-len");
                }
                return Ok(Some(len - VIRTIO_NET_HDR_LEN as u32));
            }
            unsafe { asm!("pause", options(nomem, nostack, preserves_flags)) };
        }
        Ok(None)
    }

    pub(crate) fn arm_receive(
        &mut self,
        memory: &mut NetTaskMemory<'_>,
    ) -> Result<(), &'static str> {
        let region = memory.as_mut_bytes();
        prepare_net_rx_buffer(region, self.rx)?;
        compiler_fence();
        outw(self.io_base + VIRTIO_PCI_QUEUE_NOTIFY, 0);
        Ok(())
    }

    fn find_device(&mut self) -> Result<(), &'static str> {
        for device in 0u8..32 {
            for function in 0u8..8 {
                let vendor = pci::read_u16(0, device, function, 0x00);
                if vendor == 0xffff {
                    if function == 0 {
                        break;
                    }
                    continue;
                }
                let device_id = pci::read_u16(0, device, function, 0x02);
                if vendor == 0x1af4 && device_id == 0x1000 {
                    let bar0 = pci::read_u32(0, device, function, 0x10);
                    if bar0 & 1 == 0 {
                        return Err("virtio-net-bar-not-io");
                    }
                    let command = pci::read_u16(0, device, function, 0x04);
                    pci::write_u16(0, device, function, 0x04, command | 0x0005);
                    self.io_base = (bar0 & !0x3) as u16;
                    return Ok(());
                }
            }
        }
        Err("virtio-net-pci-missing")
    }

    fn begin_init(&self) {
        outb(self.io_base + VIRTIO_PCI_STATUS, 0);
        outb(
            self.io_base + VIRTIO_PCI_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
        );
        outl(self.io_base + VIRTIO_PCI_GUEST_FEATURES, 0);
    }

    fn finish_init(&self) {
        outb(
            self.io_base + VIRTIO_PCI_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK,
        );
    }

    fn setup_queue(
        &self,
        region: &mut [u8; NET_TASK_BYTES],
        queue: u16,
        offset: usize,
    ) -> Result<NetVirtQueue, &'static str> {
        outw(self.io_base + VIRTIO_PCI_QUEUE_SEL, queue);
        let reported = inw(self.io_base + VIRTIO_PCI_QUEUE_NUM);
        if reported == 0 || reported > NET_QUEUE_CAP {
            return Err("virtio-net-queue-size");
        }
        let queue_mem = region[offset..offset + NET_VIRTQ_BYTES].as_mut_ptr() as usize;
        if queue_mem & 0xfff != 0 {
            return Err("virtio-net-queue-align");
        }
        zero_net_region(region, offset, NET_VIRTQ_BYTES);
        outl(
            self.io_base + VIRTIO_PCI_QUEUE_PFN,
            (queue_mem as u32) >> 12,
        );
        Ok(NetVirtQueue::new(offset, reported))
    }
}

pub(crate) struct NetDriverTask {
    driver: VirtioLegacyPciNet,
}

impl NetDriverTask {
    pub(crate) const fn new() -> Self {
        Self {
            driver: VirtioLegacyPciNet::new(),
        }
    }

    pub(crate) fn initialize(&mut self) -> Result<(), &'static str> {
        if !net_region_is_mmu_covered() {
            return Err("net-region-range");
        }
        protected_net_region(|memory| self.driver.init(memory))
    }

    pub(crate) fn transmit_probe(&mut self) -> Result<(), &'static str> {
        protected_net_region(|memory| self.driver.transmit_frame(memory, net_tx_probe_frame()))
    }

    pub(crate) fn transmit_reply(&mut self, reply: &NetworkReply) -> Result<(), &'static str> {
        protected_net_region(|memory| self.driver.transmit_frame(memory, reply.as_frame()))
    }

    pub(crate) fn receive_raw_frame(&mut self) -> Result<Option<Message>, &'static str> {
        self.receive_raw_frame_for(EP_TCPIP, REQUEST_TCPIP_NET)
    }

    pub(crate) fn receive_raw_frame_for(
        &mut self,
        endpoint: EndpointId,
        request: RequestId,
    ) -> Result<Option<Message>, &'static str> {
        protected_net_region(|memory| {
            let Some(raw_len) = self.driver.poll_receive_frame(memory)? else {
                return Ok(None);
            };
            memory.copy_virtio_rx_frame(raw_len)?;
            self.driver.arm_receive(memory)?;
            Ok(Some(Message::new(
                TASK_NET,
                endpoint,
                request,
                CAP_KERNEL,
                net_frame_message_body(NetworkFrameDirection::Rx, NET_RX_BUFFER_ID, raw_len)?,
            )))
        })
    }

    pub(crate) fn stage_raw_ingress(
        &mut self,
        memory: &mut NetTaskMemory<'_>,
        message: Message,
        frame: &[u8],
    ) -> Result<Message, &'static str> {
        if message.from != TASK_TCPIP || message.to != crate::EP_NET {
            return Err("net-request-route");
        }
        if message.request != REQUEST_TCPIP_NET {
            return Err("net-request-id");
        }
        match message.body {
            MessageBody::Pair(len, 0) if len as usize == frame.len() => {
                let _descriptor = NetworkFrameDescriptor::new(
                    NetworkFrameDirection::Rx,
                    NetworkFrameType::Ethernet,
                    NET_RX_BUFFER_ID,
                    frame.len(),
                    NET_TASK_BYTES - NET_FRAME_OFFSET,
                )
                .map_err(|_| "net-frame-descriptor")?;
            }
            _ => return Err("net-request-body"),
        }
        memory.put_frame(frame)?;
        Ok(Message::new(
            TASK_NET,
            EP_TCPIP,
            REQUEST_TCPIP_NET,
            CAP_KERNEL,
            MessageBody::Pair(frame.len() as u32, 0),
        ))
    }
}
