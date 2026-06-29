use core::arch::asm;
use core::mem::size_of;
use core::ptr::write_volatile;

use crate::{
    align_up, block_region_is_mmu_covered, compiler_fence, inw, outb, outl, outw, pci,
    prepare_block_read, protected_block_region, read_u16_region, write_u16_region, zero_region,
    BlockTaskMemory, Message, MessageBody, VirtqDesc, BLOCK_DATA_OFFSET, BLOCK_QUEUE_CAP,
    BLOCK_QUEUE_OFFSET, BLOCK_REQUEST_OFFSET, BLOCK_SECTOR_BYTES, BLOCK_STATUS_OFFSET,
    BLOCK_TASK_BYTES, BLOCK_VIRTQ_BYTES, BOOT_SIGNATURE_OFFSET, CAP_KERNEL, EP_FS,
    REQUEST_FS_BLOCK, TASK_BLOCK, TASK_FS, VIRTIO_PCI_GUEST_FEATURES, VIRTIO_PCI_QUEUE_NOTIFY,
    VIRTIO_PCI_QUEUE_NUM, VIRTIO_PCI_QUEUE_PFN, VIRTIO_PCI_QUEUE_SEL, VIRTIO_PCI_STATUS,
    VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DRIVER, VIRTIO_STATUS_DRIVER_OK, VIRTQ_DESC_F_WRITE,
    VRING_DESC_F_NEXT,
};

#[repr(C)]
pub(crate) struct VirtioBlkReqHeader {
    pub(crate) req_type: u32,
    pub(crate) reserved: u32,
    pub(crate) sector: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct VirtQueue {
    offset: usize,
    size: u16,
}

impl VirtQueue {
    pub(crate) const fn new(offset: usize, size: u16) -> Self {
        Self { offset, size }
    }

    pub(crate) fn submit_block_read(
        self,
        region: &mut [u8; BLOCK_TASK_BYTES],
    ) -> Result<(), &'static str> {
        if self.size < 3 {
            return Err("virtio-blk-queue-small");
        }
        let request_addr = region[BLOCK_REQUEST_OFFSET..].as_ptr() as u64;
        let data_addr = region[BLOCK_DATA_OFFSET..].as_ptr() as u64;
        let status_addr = region[BLOCK_STATUS_OFFSET..].as_ptr() as u64;
        self.write_desc(
            region,
            0,
            VirtqDesc {
                addr: request_addr,
                len: size_of::<VirtioBlkReqHeader>() as u32,
                flags: VRING_DESC_F_NEXT,
                next: 1,
            },
        );
        self.write_desc(
            region,
            1,
            VirtqDesc {
                addr: data_addr,
                len: BLOCK_SECTOR_BYTES as u32,
                flags: VRING_DESC_F_NEXT | VIRTQ_DESC_F_WRITE,
                next: 2,
            },
        );
        self.write_desc(
            region,
            2,
            VirtqDesc {
                addr: status_addr,
                len: 1,
                flags: VIRTQ_DESC_F_WRITE,
                next: 0,
            },
        );
        self.write_avail(region, 0);
        Ok(())
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn submit_block_write(
        self,
        region: &mut [u8; BLOCK_TASK_BYTES],
    ) -> Result<(), &'static str> {
        if self.size < 3 {
            return Err("virtio-blk-queue-small");
        }
        let request_addr = region[BLOCK_REQUEST_OFFSET..].as_ptr() as u64;
        let data_addr = region[BLOCK_DATA_OFFSET..].as_ptr() as u64;
        let status_addr = region[BLOCK_STATUS_OFFSET..].as_ptr() as u64;
        self.write_desc(
            region,
            0,
            VirtqDesc {
                addr: request_addr,
                len: size_of::<VirtioBlkReqHeader>() as u32,
                flags: VRING_DESC_F_NEXT,
                next: 1,
            },
        );
        self.write_desc(
            region,
            1,
            VirtqDesc {
                addr: data_addr,
                len: BLOCK_SECTOR_BYTES as u32,
                flags: VRING_DESC_F_NEXT,
                next: 2,
            },
        );
        self.write_desc(
            region,
            2,
            VirtqDesc {
                addr: status_addr,
                len: 1,
                flags: VIRTQ_DESC_F_WRITE,
                next: 0,
            },
        );
        self.write_avail(region, 0);
        Ok(())
    }

    pub(crate) fn write_desc(
        self,
        region: &mut [u8; BLOCK_TASK_BYTES],
        index: usize,
        desc: VirtqDesc,
    ) {
        let ptr = region[self.offset..].as_mut_ptr() as *mut VirtqDesc;
        unsafe { write_volatile(ptr.add(index), desc) };
    }

    pub(crate) fn write_avail(self, region: &mut [u8; BLOCK_TASK_BYTES], desc_index: u16) {
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

    pub(crate) fn used_idx(self, region: &[u8; BLOCK_TASK_BYTES]) -> u16 {
        let used = align_up(
            self.offset + size_of::<VirtqDesc>() * self.size as usize + 4 + self.size as usize * 2,
            4096,
        );
        read_u16_region(region, used + 2)
    }
}

pub(crate) struct VirtioLegacyPciBlock {
    io_base: u16,
    queue: VirtQueue,
    used_seen: u16,
}

pub(crate) struct BlockDriverTask {
    driver: VirtioLegacyPciBlock,
}

impl BlockDriverTask {
    pub(crate) const fn new() -> Self {
        Self {
            driver: VirtioLegacyPciBlock::new(),
        }
    }

    pub(crate) fn initialize(&mut self) -> Result<(), &'static str> {
        if !block_region_is_mmu_covered() {
            return Err("block-region-range");
        }
        protected_block_region(|memory| self.driver.init(memory))
    }

    pub(crate) fn read_sector_reply(&mut self, message: Message) -> Result<Message, &'static str> {
        let lba = match message.body {
            MessageBody::Pair(lba, 1) => lba,
            _ => return Err("block-request-body"),
        };
        if message.from != TASK_FS {
            return Err("block-request");
        }
        protected_block_region(|memory| {
            self.driver.read_sector(memory, u64::from(lba))?;
            if lba == 0
                && (memory.as_mut_bytes()[BLOCK_DATA_OFFSET + BOOT_SIGNATURE_OFFSET] != 0x55
                    || memory.as_mut_bytes()[BLOCK_DATA_OFFSET + BOOT_SIGNATURE_OFFSET + 1] != 0xaa)
            {
                return Err("block-sector0-signature");
            }
            Ok(())
        })?;
        Ok(Message::new(
            TASK_BLOCK,
            EP_FS,
            REQUEST_FS_BLOCK,
            CAP_KERNEL,
            MessageBody::Pair(lba, BLOCK_SECTOR_BYTES as u32),
        ))
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn write_sector_reply(&mut self, message: Message) -> Result<Message, &'static str> {
        let lba = match message.body {
            MessageBody::Pair(lba, 2) => lba,
            _ => return Err("block-write-request-body"),
        };
        if message.from != TASK_FS {
            return Err("block-write-request");
        }
        protected_block_region(|memory| self.driver.write_sector(memory, u64::from(lba)))?;
        Ok(Message::new(
            TASK_BLOCK,
            EP_FS,
            REQUEST_FS_BLOCK,
            CAP_KERNEL,
            MessageBody::Pair(lba, BLOCK_SECTOR_BYTES as u32),
        ))
    }
}

impl VirtioLegacyPciBlock {
    pub(crate) const fn new() -> Self {
        Self {
            io_base: 0,
            queue: VirtQueue::new(BLOCK_QUEUE_OFFSET, 0),
            used_seen: 0,
        }
    }

    pub(crate) fn init(&mut self, memory: &mut BlockTaskMemory<'_>) -> Result<(), &'static str> {
        let region = memory.as_mut_bytes();
        self.find_device()?;
        self.begin_init();
        self.queue = self.setup_queue(region, 0, BLOCK_QUEUE_OFFSET)?;
        self.used_seen = self.queue.used_idx(region);
        self.finish_init();
        Ok(())
    }

    pub(crate) fn read_sector(
        &mut self,
        memory: &mut BlockTaskMemory<'_>,
        sector: u64,
    ) -> Result<(), &'static str> {
        let region = memory.as_mut_bytes();
        prepare_block_read(region, self.queue, sector)?;
        compiler_fence();
        outw(self.io_base + VIRTIO_PCI_QUEUE_NOTIFY, 0);

        for _ in 0..1_000_000 {
            compiler_fence();
            let used = self.queue.used_idx(region);
            if used != self.used_seen {
                self.used_seen = used;
                if region[BLOCK_STATUS_OFFSET] == crate::VIRTIO_BLK_S_OK {
                    return Ok(());
                }
                return Err("virtio-blk-status");
            }
            unsafe { asm!("pause", options(nomem, nostack, preserves_flags)) };
        }
        Err("virtio-blk-timeout")
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn write_sector(
        &mut self,
        memory: &mut BlockTaskMemory<'_>,
        sector: u64,
    ) -> Result<(), &'static str> {
        let region = memory.as_mut_bytes();
        crate::prepare_block_write(region, self.queue, sector)?;
        compiler_fence();
        outw(self.io_base + VIRTIO_PCI_QUEUE_NOTIFY, 0);

        for _ in 0..1_000_000 {
            compiler_fence();
            let used = self.queue.used_idx(region);
            if used != self.used_seen {
                self.used_seen = used;
                if region[BLOCK_STATUS_OFFSET] == crate::VIRTIO_BLK_S_OK {
                    return Ok(());
                }
                return Err("virtio-blk-write-status");
            }
            unsafe { asm!("pause", options(nomem, nostack, preserves_flags)) };
        }
        Err("virtio-blk-write-timeout")
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
                if vendor == 0x1af4 && device_id == 0x1001 {
                    let bar0 = pci::read_u32(0, device, function, 0x10);
                    if bar0 & 1 == 0 {
                        return Err("virtio-blk-bar-not-io");
                    }
                    let command = pci::read_u16(0, device, function, 0x04);
                    pci::write_u16(0, device, function, 0x04, command | 0x0005);
                    self.io_base = (bar0 & !0x3) as u16;
                    return Ok(());
                }
            }
        }
        Err("virtio-blk-pci-missing")
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
        region: &mut [u8; BLOCK_TASK_BYTES],
        queue: u16,
        offset: usize,
    ) -> Result<VirtQueue, &'static str> {
        outw(self.io_base + VIRTIO_PCI_QUEUE_SEL, queue);
        let reported = inw(self.io_base + VIRTIO_PCI_QUEUE_NUM);
        if reported == 0 || reported > BLOCK_QUEUE_CAP {
            return Err("virtio-blk-queue-size");
        }
        let queue_mem = region[offset..offset + BLOCK_VIRTQ_BYTES].as_mut_ptr() as usize;
        if queue_mem & 0xfff != 0 {
            return Err("virtio-blk-queue-align");
        }
        zero_region(region, offset, BLOCK_VIRTQ_BYTES);
        outl(
            self.io_base + VIRTIO_PCI_QUEUE_PFN,
            (queue_mem as u32) >> 12,
        );
        Ok(VirtQueue::new(offset, reported))
    }
}
