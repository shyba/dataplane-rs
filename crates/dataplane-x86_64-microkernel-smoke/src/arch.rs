use core::arch::{asm, global_asm};
use core::ptr::{addr_of_mut, read_volatile, write_volatile};

use crate::fat32::FsTaskMemory;
use crate::{
    fail, halt_forever, BlockTaskMemory, NetTaskMemory, BLOCK_TASK_BYTES, FS_TASK_BYTES,
    NET_TASK_BYTES,
};

global_asm!(
    r#"
.global _start
_start:
    cld
    call rust_main
1:
    hlt
    jmp 1b

.global page_fault_entry
page_fault_entry:
    cld
    mov rdi, rsp
    call page_fault_handler
    add rsp, 8
    iretq

.global trigger_task_fault
.global task_fault_probe
.global task_fault_resume
trigger_task_fault:
task_fault_probe:
    mov byte ptr [rdi], 0x5a
task_fault_resume:
    ret
"#
);

#[repr(align(4096))]
struct TaskRegion([u8; BLOCK_TASK_BYTES]);

#[repr(align(4096))]
struct FsRegion([u8; FS_TASK_BYTES]);

#[repr(align(4096))]
struct NetRegion([u8; NET_TASK_BYTES]);

#[repr(align(4096))]
struct PageTable([u64; 512]);

static mut BLOCK_TASK_REGION: TaskRegion = TaskRegion([0; BLOCK_TASK_BYTES]);
static mut FS_TASK_REGION: FsRegion = FsRegion([0; FS_TASK_BYTES]);
static mut NET_TASK_REGION: NetRegion = NetRegion([0; NET_TASK_BYTES]);
static mut PML4: PageTable = PageTable([0; 512]);
static mut PDPT: PageTable = PageTable([0; 512]);
static mut PD0: PageTable = PageTable([0; 512]);
static mut PD1: PageTable = PageTable([0; 512]);
static mut PD2: PageTable = PageTable([0; 512]);
static mut PD3: PageTable = PageTable([0; 512]);
static mut PT0: PageTable = PageTable([0; 512]);
static mut IDT: [IdtEntry; 256] = [IdtEntry::missing(); 256];
static mut TASK_FAULT_SEEN: bool = false;

extern "C" {
    fn trigger_task_fault(addr: *mut u8);
    static task_fault_probe: u8;
    static task_fault_resume: u8;
}

pub(crate) unsafe fn init_interrupts_and_mmu() {
    idt::init();
    mmu::install_identity_with_task_pages();
}

pub(crate) unsafe fn allow_fs_region_for_nested_read() {
    mmu::allow_region(fs_region_addr() as usize, FS_TASK_BYTES);
}

pub(crate) fn protected_block_region<F, R>(f: F) -> R
where
    F: FnOnce(&mut BlockTaskMemory<'_>) -> R,
{
    unsafe {
        let addr = block_region_addr() as usize;
        mmu::allow_region(addr, BLOCK_TASK_BYTES);
        let mut memory = BlockTaskMemory {
            bytes: &mut (*addr_of_mut!(BLOCK_TASK_REGION)).0,
        };
        let result = f(&mut memory);
        mmu::deny_region(addr, BLOCK_TASK_BYTES);
        result
    }
}

pub(crate) fn protected_fs_region<F, R>(f: F) -> R
where
    F: FnOnce(&mut FsTaskMemory<'_>) -> R,
{
    unsafe {
        let addr = fs_region_addr() as usize;
        mmu::allow_region(addr, FS_TASK_BYTES);
        let mut memory = FsTaskMemory {
            bytes: &mut (*addr_of_mut!(FS_TASK_REGION)).0,
        };
        let result = f(&mut memory);
        mmu::deny_region(addr, FS_TASK_BYTES);
        result
    }
}

pub(crate) fn protected_net_region<F, R>(f: F) -> R
where
    F: FnOnce(&mut NetTaskMemory<'_>) -> R,
{
    unsafe {
        let addr = net_region_addr() as usize;
        mmu::allow_region(addr, NET_TASK_BYTES);
        let mut memory = NetTaskMemory {
            bytes: &mut (*addr_of_mut!(NET_TASK_REGION)).0,
        };
        let result = f(&mut memory);
        mmu::deny_region(addr, NET_TASK_BYTES);
        result
    }
}

pub(crate) fn with_block_and_fs_regions<F, R>(f: F) -> R
where
    F: FnOnce(&mut [u8; BLOCK_TASK_BYTES], &mut [u8; FS_TASK_BYTES]) -> R,
{
    unsafe {
        let block_addr = block_region_addr() as usize;
        let fs_addr = fs_region_addr() as usize;
        mmu::allow_region(block_addr, BLOCK_TASK_BYTES);
        mmu::allow_region(fs_addr, FS_TASK_BYTES);
        let block = &mut (*addr_of_mut!(BLOCK_TASK_REGION)).0;
        let fs = &mut (*addr_of_mut!(FS_TASK_REGION)).0;
        let result = f(block, fs);
        mmu::deny_region(fs_addr, FS_TASK_BYTES);
        mmu::deny_region(block_addr, BLOCK_TASK_BYTES);
        result
    }
}

fn block_region_addr() -> *mut u8 {
    addr_of_mut!(BLOCK_TASK_REGION) as *mut u8
}

fn fs_region_addr() -> *mut u8 {
    addr_of_mut!(FS_TASK_REGION) as *mut u8
}

fn net_region_addr() -> *mut u8 {
    addr_of_mut!(NET_TASK_REGION) as *mut u8
}

pub(crate) fn prove_fault_containment() -> bool {
    unsafe {
        if !block_region_is_mmu_covered() {
            return false;
        }
        protected_block_region(|_| {});
        trigger_task_fault(block_region_addr());
        TASK_FAULT_SEEN
    }
}

pub(crate) fn block_region_is_mmu_covered() -> bool {
    (block_region_addr() as usize + BLOCK_TASK_BYTES) <= 2 * 1024 * 1024
}

pub(crate) fn fs_region_is_mmu_covered() -> bool {
    (fs_region_addr() as usize + FS_TASK_BYTES) <= 2 * 1024 * 1024
}

pub(crate) fn net_region_is_mmu_covered() -> bool {
    (net_region_addr() as usize + NET_TASK_BYTES) <= 2 * 1024 * 1024
}

/// Real narrow MMU access proof for the net task region, mirroring
/// `prove_fault_containment`: deny the whole net region, touch it through
/// `trigger_task_fault` so the page-fault handler observes the access, confirm
/// the fault was seen, then restore access and clear the latch. A `true`
/// result means the deny/fault/restore cycle actually executed, not merely
/// that the region sits below the 2 MiB identity-mapped range.
pub(crate) fn prove_net_region_fault_containment() -> bool {
    unsafe {
        if !net_region_is_mmu_covered() {
            return false;
        }
        let addr = net_region_addr() as usize;
        TASK_FAULT_SEEN = false;
        mmu::deny_region(addr, NET_TASK_BYTES);
        trigger_task_fault(net_region_addr());
        let seen = TASK_FAULT_SEEN;
        mmu::allow_region(addr, NET_TASK_BYTES);
        TASK_FAULT_SEEN = false;
        seen
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    fail("panic")
}

#[no_mangle]
extern "C" fn page_fault_handler(frame: *mut u64) {
    unsafe {
        let rip_slot = frame.add(1);
        let rip = read_volatile(rip_slot);
        let probe = (&task_fault_probe as *const u8) as u64;
        if rip == probe {
            // This handler only resumes after the faulting probe instruction.
            // General-purpose register preservation is not part of this proof.
            TASK_FAULT_SEEN = true;
            write_volatile(rip_slot, (&task_fault_resume as *const u8) as u64);
            return;
        }
    }

    crate::serial::write_str("DPMK:FAIL:unexpected-page-fault\n");
    halt_forever()
}

mod idt {
    use super::{addr_of_mut, write_volatile, IdtEntry, IdtPointer, IDT};

    extern "C" {
        static page_fault_entry: u8;
    }

    pub unsafe fn init() {
        let handler = (&page_fault_entry as *const u8) as u64;
        write_volatile(addr_of_mut!(IDT[14]), IdtEntry::new(handler, 0x18, 0x8e00));
        let ptr = IdtPointer {
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: (addr_of_mut!(IDT) as *const _) as u64,
        };
        core::arch::asm!("lidt [{}]", in(reg) &ptr, options(readonly, nostack, preserves_flags));
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    options: u16,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            options: 0,
            offset_mid: 0,
            offset_high: 0,
            zero: 0,
        }
    }

    const fn new(handler: u64, selector: u16, options: u16) -> Self {
        Self {
            offset_low: handler as u16,
            selector,
            options,
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            zero: 0,
        }
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

mod mmu {
    use super::{addr_of_mut, asm, PageTable, PD0, PD1, PD2, PD3, PDPT, PML4, PT0};

    const PRESENT: u64 = 1;
    const WRITABLE: u64 = 1 << 1;
    const HUGE: u64 = 1 << 7;
    const PAGE: usize = 4096;

    pub unsafe fn install_identity_with_task_pages() {
        clear(addr_of_mut!(PML4));
        clear(addr_of_mut!(PDPT));
        clear(addr_of_mut!(PD0));
        clear(addr_of_mut!(PD1));
        clear(addr_of_mut!(PD2));
        clear(addr_of_mut!(PD3));
        clear(addr_of_mut!(PT0));

        (*addr_of_mut!(PML4)).0[0] = table(addr_of_mut!(PDPT));
        (*addr_of_mut!(PDPT)).0[0] = table(addr_of_mut!(PD0));
        (*addr_of_mut!(PDPT)).0[1] = table(addr_of_mut!(PD1));
        (*addr_of_mut!(PDPT)).0[2] = table(addr_of_mut!(PD2));
        (*addr_of_mut!(PDPT)).0[3] = table(addr_of_mut!(PD3));
        (*addr_of_mut!(PD0)).0[0] = table(addr_of_mut!(PT0));

        for index in 0..512 {
            (*addr_of_mut!(PT0)).0[index] = (index as u64 * PAGE as u64) | PRESENT | WRITABLE;
        }
        fill_huge(addr_of_mut!(PD0), 1, 0);
        fill_huge(addr_of_mut!(PD1), 0, 0x4000_0000);
        fill_huge(addr_of_mut!(PD2), 0, 0x8000_0000);
        fill_huge(addr_of_mut!(PD3), 0, 0xc000_0000);

        let cr3 = addr_of_mut!(PML4) as u64;
        asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags));
    }

    pub unsafe fn allow_region(addr: usize, len: usize) {
        set_region(addr, len, true);
    }

    pub unsafe fn deny_region(addr: usize, len: usize) {
        set_region(addr, len, false);
    }

    unsafe fn set_region(addr: usize, len: usize, present: bool) {
        let start = addr & !(PAGE - 1);
        let end = (addr + len + PAGE - 1) & !(PAGE - 1);
        let mut page = start;
        while page < end {
            let index = page / PAGE;
            if index < 512 {
                let entry = &mut (*addr_of_mut!(PT0)).0[index];
                if present {
                    *entry |= PRESENT;
                } else {
                    *entry &= !PRESENT;
                }
                asm!("invlpg [{}]", in(reg) page, options(nostack, preserves_flags));
            }
            page += PAGE;
        }
    }

    unsafe fn clear(table: *mut PageTable) {
        for entry in &mut (*table).0 {
            *entry = 0;
        }
    }

    fn table(table: *mut PageTable) -> u64 {
        table as u64 | PRESENT | WRITABLE
    }

    unsafe fn fill_huge(table: *mut PageTable, start_index: usize, base: u64) {
        for index in start_index..512 {
            (*table).0[index] = (base + index as u64 * 0x20_0000) | PRESENT | WRITABLE | HUGE;
        }
    }
}

#[inline(always)]
pub(crate) fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

#[inline(always)]
pub(crate) fn compiler_fence() {
    // SAFETY: an empty asm block without `nomem` is a compiler-only memory barrier.
    unsafe {
        asm!("", options(nostack, preserves_flags));
    }
}

pub(crate) mod pci {
    use super::{inl, outl};

    const CONFIG_ADDRESS: u16 = 0xcf8;
    const CONFIG_DATA: u16 = 0xcfc;

    pub fn read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
        let address = 0x8000_0000u32
            | (u32::from(bus) << 16)
            | (u32::from(device) << 11)
            | (u32::from(function) << 8)
            | u32::from(offset & 0xfc);
        outl(CONFIG_ADDRESS, address);
        inl(CONFIG_DATA)
    }

    pub fn read_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
        let value = read_u32(bus, device, function, offset);
        let shift = u32::from(offset & 0x02) * 8;
        ((value >> shift) & 0xffff) as u16
    }

    pub fn write_u16(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
        let mut current = read_u32(bus, device, function, offset);
        let shift = u32::from(offset & 0x02) * 8;
        current &= !(0xffff << shift);
        current |= u32::from(value) << shift;
        let address = 0x8000_0000u32
            | (u32::from(bus) << 16)
            | (u32::from(device) << 11)
            | (u32::from(function) << 8)
            | u32::from(offset & 0xfc);
        outl(CONFIG_ADDRESS, address);
        outl(CONFIG_DATA, current);
    }
}

#[inline(always)]
pub(crate) fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags))
    };
}

#[inline(always)]
pub(crate) fn outw(port: u16, value: u16) {
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags))
    };
}

#[inline(always)]
pub(crate) fn outl(port: u16, value: u32) {
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags))
    };
}

#[inline(always)]
pub(crate) fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags))
    };
    value
}

#[inline(always)]
pub(crate) fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack, preserves_flags))
    };
    value
}

#[inline(always)]
fn inl(port: u16) -> u32 {
    let value: u32;
    unsafe {
        asm!("in eax, dx", out("eax") value, in("dx") port, options(nomem, nostack, preserves_flags))
    };
    value
}
