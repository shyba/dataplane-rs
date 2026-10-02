use core::mem::size_of;
use core::ptr::{read_volatile, write_volatile};

use dataplane_microkernel_core::{
    EthernetFrameSpec, FixedNetworkDriver, NetworkFrameDescriptor, NetworkFrameDirection,
    NetworkFrameType, ReceivedFrame,
};

use crate::io::{align_up, compiler_fence, inw, outl, outw};
use crate::network::{
    make_tx_frame, FailReason, NetworkTaskMemory, NET_RX_BUFFER_ID, NET_TASK_BYTES,
    NET_TX_BUFFER_ID, RX_BUFFER_OFFSET, TX_BUFFER_OFFSET, TX_FRAME_LEN, TX_PACKET_LEN,
    VIRTIO_NET_HDR_LEN,
};
use crate::pci;

pub(crate) const NET_QUEUE_BYTES: usize = 16 * 1024;
pub(crate) const RX_QUEUE_OFFSET: usize = 0;
pub(crate) const TX_QUEUE_OFFSET: usize = 16 * 1024;
pub(crate) const QUEUE_CAP: u16 = 256;

const VIRTIO_PCI_GUEST_FEATURES: u16 = 0x04;
const VIRTIO_PCI_QUEUE_PFN: u16 = 0x08;
const VIRTIO_PCI_QUEUE_NUM: u16 = 0x0c;
const VIRTIO_PCI_QUEUE_SEL: u16 = 0x0e;
const VIRTIO_PCI_QUEUE_NOTIFY: u16 = 0x10;
const VIRTIO_PCI_STATUS: u16 = 0x12;
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 0x01;
const VIRTIO_STATUS_DRIVER: u8 = 0x02;
const VIRTIO_STATUS_DRIVER_OK: u8 = 0x04;
const VIRTQ_DESC_F_WRITE: u16 = 2;

pub(crate) struct VirtioLegacyPciNet {
    io_base: u16,
    rx: VirtQueue,
    tx: VirtQueue,
    rx_used_seen: u16,
    tx_used_seen: u16,
}

impl VirtioLegacyPciNet {
    pub(crate) const fn new() -> Self {
        Self {
            io_base: 0,
            rx: VirtQueue::new(RX_QUEUE_OFFSET, 0),
            tx: VirtQueue::new(TX_QUEUE_OFFSET, 0),
            rx_used_seen: 0,
            tx_used_seen: 0,
        }
    }

    fn find_device(&mut self) -> Result<(), FailReason> {
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
                        return Err(FailReason("virtio-bar-not-io"));
                    }
                    let command = pci::read_u16(0, device, function, 0x04);
                    pci::write_u16(0, device, function, 0x04, command | 0x0005);
                    self.io_base = (bar0 & !0x3) as u16;
                    return Ok(());
                }
            }
        }
        Err(FailReason("virtio-net-pci-missing"))
    }

    fn begin_init(&self) {
        outw(self.io_base + VIRTIO_PCI_STATUS, 0);
        outw(
            self.io_base + VIRTIO_PCI_STATUS,
            u16::from(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER),
        );
        outl(self.io_base + VIRTIO_PCI_GUEST_FEATURES, 0);
    }

    fn finish_init(&self) {
        outw(
            self.io_base + VIRTIO_PCI_STATUS,
            u16::from(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK),
        );
    }

    fn setup_queue(
        &self,
        region: &mut [u8; NET_TASK_BYTES],
        queue: u16,
        offset: usize,
    ) -> Result<VirtQueue, FailReason> {
        outw(self.io_base + VIRTIO_PCI_QUEUE_SEL, queue);
        let reported = inw(self.io_base + VIRTIO_PCI_QUEUE_NUM);
        if reported == 0 || reported > QUEUE_CAP {
            return Err(FailReason("virtqueue-size"));
        }
        let queue_mem = region[offset..offset + NET_QUEUE_BYTES].as_mut_ptr().addr();
        if queue_mem & 0xfff != 0 {
            return Err(FailReason("virtqueue-align"));
        }
        zero_region(region, offset, NET_QUEUE_BYTES);
        outl(
            self.io_base + VIRTIO_PCI_QUEUE_PFN,
            (queue_mem as u32) >> 12,
        );
        Ok(VirtQueue::new(offset, reported))
    }
}

impl<'a> FixedNetworkDriver<NetworkTaskMemory<'a>, FailReason> for VirtioLegacyPciNet {
    fn init(&mut self, memory: &mut NetworkTaskMemory<'_>) -> Result<(), FailReason> {
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

    fn arm_receive(&mut self, memory: &mut NetworkTaskMemory<'_>) -> Result<(), FailReason> {
        let region = memory.as_mut_bytes();
        zero_region(region, RX_BUFFER_OFFSET, 2048);
        self.rx.submit_writable(region, RX_BUFFER_OFFSET, 2048)?;
        compiler_fence();
        outw(self.io_base + VIRTIO_PCI_QUEUE_NOTIFY, 0);
        Ok(())
    }

    fn transmit_frame(
        &mut self,
        memory: &mut NetworkTaskMemory<'_>,
        frame: EthernetFrameSpec<'_>,
    ) -> Result<(), FailReason> {
        let _descriptor = NetworkFrameDescriptor::new(
            NetworkFrameDirection::Tx,
            NetworkFrameType::Ethernet,
            NET_TX_BUFFER_ID,
            frame.padded_len(TX_FRAME_LEN),
            TX_FRAME_LEN,
        )
        .map_err(|_| FailReason("net-tx-descriptor"))?;
        let region = memory.as_mut_bytes();
        make_tx_frame(region, frame)?;
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
            unsafe { core::arch::asm!("pause", options(nomem, nostack, preserves_flags)) };
        }
        Err(FailReason("virtio-tx-timeout"))
    }

    fn receive_frame(
        &mut self,
        memory: &mut NetworkTaskMemory<'_>,
    ) -> Result<ReceivedFrame, FailReason> {
        let region = memory.as_mut_bytes();
        for _ in 0..1_000_000 {
            compiler_fence();
            let used = self.rx.used_idx(region);
            if used != self.rx_used_seen {
                let len = self.rx.used_len(region, self.rx_used_seen);
                self.rx_used_seen = used;
                if len <= VIRTIO_NET_HDR_LEN as u32 {
                    return Err(FailReason("virtio-rx-short"));
                }
                let transport_len = len;
                return Ok(ReceivedFrame {
                    descriptor: NetworkFrameDescriptor::new(
                        NetworkFrameDirection::Rx,
                        NetworkFrameType::Ethernet,
                        NET_RX_BUFFER_ID,
                        (len as usize) - VIRTIO_NET_HDR_LEN,
                        2048 - VIRTIO_NET_HDR_LEN,
                    )
                    .map_err(|_| FailReason("net-rx-descriptor"))?,
                    transport_len,
                });
            }
            unsafe { core::arch::asm!("pause", options(nomem, nostack, preserves_flags)) };
        }
        Err(FailReason("host-challenge-timeout"))
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[allow(dead_code)]
#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct VirtQueue {
    pub(crate) offset: usize,
    size: u16,
}

impl VirtQueue {
    const fn new(offset: usize, size: u16) -> Self {
        Self { offset, size }
    }

    fn submit_read_only(
        self,
        region: &mut [u8; NET_TASK_BYTES],
        buffer_offset: usize,
        len: usize,
    ) -> Result<(), FailReason> {
        let desc = VirtqDesc {
            addr: region[buffer_offset..].as_ptr().addr() as u64,
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
    ) -> Result<(), FailReason> {
        let desc = VirtqDesc {
            addr: region[buffer_offset..].as_ptr().addr() as u64,
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
        unsafe {
            write_volatile(ptr.add(index), desc);
        }
    }

    fn write_avail(self, region: &mut [u8; NET_TASK_BYTES], desc_index: u16) {
        let avail = self.offset + size_of::<VirtqDesc>() * self.size as usize;
        write_u16_region(region, avail, 0);
        let idx = read_u16_region(region, avail + 2);
        write_u16_region(
            region,
            avail + 4 + (idx as usize % self.size as usize) * 2,
            desc_index,
        );
        compiler_fence();
        write_u16_region(region, avail + 2, idx.wrapping_add(1));
    }

    fn used_idx(self, region: &[u8; NET_TASK_BYTES]) -> u16 {
        let used = align_up(
            self.offset + size_of::<VirtqDesc>() * self.size as usize + 4 + self.size as usize * 2,
            4096,
        );
        read_u16_region(region, used + 2)
    }

    fn used_len(self, region: &[u8; NET_TASK_BYTES], previous_idx: u16) -> u32 {
        let used = align_up(
            self.offset + size_of::<VirtqDesc>() * self.size as usize + 4 + self.size as usize * 2,
            4096,
        );
        let entry = used + 4 + (previous_idx as usize % self.size as usize) * 8;
        read_u32_region(region, entry + 4)
    }
}

fn zero_region(region: &mut [u8; NET_TASK_BYTES], offset: usize, len: usize) {
    for byte in &mut region[offset..offset + len] {
        *byte = 0;
    }
}

// Queue memory is shared with the device. Access each ring field as one
// volatile word, not separate byte loads/stores that can tear an index update.
// setup_queue checks page alignment; every caller uses an aligned, in-bounds
// virtqueue field offset. x86 is little-endian. Fences at publication/polling
// sites remain necessary: volatile access alone is not a memory barrier.
fn write_u16_region(region: &mut [u8; NET_TASK_BYTES], offset: usize, value: u16) {
    unsafe { write_volatile(region.as_mut_ptr().add(offset).cast::<u16>(), value) };
}

fn read_u16_region(region: &[u8; NET_TASK_BYTES], offset: usize) -> u16 {
    unsafe { read_volatile(region.as_ptr().add(offset).cast::<u16>()) }
}

fn read_u32_region(region: &[u8; NET_TASK_BYTES], offset: usize) -> u32 {
    unsafe { read_volatile(region.as_ptr().add(offset).cast::<u32>()) }
}
