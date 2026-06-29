use core::arch::asm;

use crate::config::{Page, ProtectedShard, ShardId, SHARD_REGION_SIZE};

const DESC_VALID: u64 = 1 << 0;
const DESC_TABLE: u64 = 1 << 1;
const DESC_AF: u64 = 1 << 10;
const DESC_AP_RW_EL2: u64 = 0 << 6;
const DESC_AP_NO_ACCESS: u64 = 1 << 6;
const DESC_INNER_SHAREABLE: u64 = 3 << 8;
const ATTR_NORMAL: u64 = 0 << 2;
const ATTR_DEVICE: u64 = 1 << 2;
const ATTR_NORMAL_NC: u64 = 2 << 2;
const ATTR_INDEX_NORMAL: u64 = 0xff;
const ATTR_INDEX_DEVICE: u64 = 0x00 << 8;
const ATTR_INDEX_NORMAL_NC: u64 = 0x44 << 16;
const BLOCK_SIZE: usize = 2 * 1024 * 1024;
const PAGE_SIZE: usize = 4096;

const FORBIDDEN_SHARD: usize = crate::config::FORBIDDEN_SHARD;

#[repr(align(4096))]
struct ShardMemory([u8; SHARD_REGION_SIZE]);

static mut L1_TABLE: Page = Page([0; 512]);
static mut L2_TABLE: Page = Page([0; 512]);
static mut L3_LOW_TABLE: Page = Page([0; 512]);
static mut SHARD_MEMORY: [ShardMemory; crate::config::REGION_COUNT] = [
    ShardMemory([0; SHARD_REGION_SIZE]),
    ShardMemory([0; SHARD_REGION_SIZE]),
    ShardMemory([0; SHARD_REGION_SIZE]),
    ShardMemory([0; SHARD_REGION_SIZE]),
];

static mut FAULT_SEEN: bool = false;
static mut FAULT_ELR: u64 = 0;
static mut FAULT_ESR: u64 = 0;

impl ProtectedShard {
    pub(crate) fn with_access<R>(&self, f: impl FnOnce(&mut [u8; SHARD_REGION_SIZE]) -> R) -> R {
        allow_shard(self.id);
        let result = f(unsafe { &mut SHARD_MEMORY[self.id.0].0 });
        deny_shard(self.id);
        result
    }
}

pub(crate) fn init() {
    unsafe {
        zero_table(core::ptr::addr_of_mut!(L1_TABLE));
        zero_table(core::ptr::addr_of_mut!(L2_TABLE));
        zero_table(core::ptr::addr_of_mut!(L3_LOW_TABLE));

        (*core::ptr::addr_of_mut!(L1_TABLE)).0[0] = table_desc(core::ptr::addr_of!(L2_TABLE));
        (*core::ptr::addr_of_mut!(L2_TABLE)).0[0] = table_desc(core::ptr::addr_of!(L3_LOW_TABLE));
        let mut page = 0usize;
        while page < 512 {
            let addr = page * PAGE_SIZE;
            (*core::ptr::addr_of_mut!(L3_LOW_TABLE)).0[page] =
                page_desc(addr as u64, ATTR_NORMAL, DESC_AP_RW_EL2);
            page += 1;
        }
        let mut block = 0usize;
        while block < 512 {
            let addr = block * BLOCK_SIZE;
            let attr = if addr >= 0x3f00_0000 {
                ATTR_DEVICE
            } else {
                ATTR_NORMAL
            };
            if block != 0 {
                (*core::ptr::addr_of_mut!(L2_TABLE)).0[block] =
                    block_desc(addr as u64, attr, DESC_AP_RW_EL2);
            }
            block += 1;
        }

        install_translation_tables();
        deny_shard(ShardId(FORBIDDEN_SHARD));
    }
}

pub(crate) fn allow_shard(id: ShardId) {
    set_shard_access(id, DESC_AP_RW_EL2);
}

pub(crate) fn deny_shard(id: ShardId) {
    set_shard_access(id, DESC_AP_NO_ACCESS);
}

pub(crate) fn mark_fault(esr: u64, elr: u64) {
    unsafe {
        FAULT_SEEN = true;
        FAULT_ESR = esr;
        FAULT_ELR = elr;
    }
}

pub(crate) fn seen_fault() -> bool {
    unsafe { FAULT_SEEN }
}

pub(crate) fn trigger_forbidden_access(shard: ProtectedShard) {
    deny_shard(shard.id);
    unsafe {
        let ptr = SHARD_MEMORY[shard.id.0].0.as_mut_ptr();
        core::ptr::write_volatile(ptr, 0xaa);
    }
}

fn set_shard_access(id: ShardId, ap: u64) {
    unsafe {
        let shard_base = core::ptr::addr_of!(SHARD_MEMORY) as usize + id.0 * SHARD_REGION_SIZE;
        assert!(shard_base % PAGE_SIZE == 0);
        let page_index = shard_base / PAGE_SIZE;
        assert!(page_index < 512);
        (*core::ptr::addr_of_mut!(L3_LOW_TABLE)).0[page_index] = if ap == DESC_AP_NO_ACCESS {
            0
        } else {
            page_desc(shard_base as u64, ATTR_NORMAL_NC, ap)
        };
        tlbi();
    }
}

unsafe fn zero_table(table: *mut Page) {
    let mut i = 0usize;
    while i < (*table).0.len() {
        (*table).0[i] = 0;
        i += 1;
    }
}

fn table_desc(table: *const Page) -> u64 {
    table as u64 | DESC_VALID | DESC_TABLE
}

fn block_desc(addr: u64, attr: u64, ap: u64) -> u64 {
    (addr & 0x0000_ffff_ffe0_0000) | DESC_VALID | DESC_AF | DESC_INNER_SHAREABLE | attr | ap
}

fn page_desc(addr: u64, attr: u64, ap: u64) -> u64 {
    (addr & 0x0000_ffff_ffff_f000)
        | DESC_VALID
        | DESC_TABLE
        | DESC_AF
        | DESC_INNER_SHAREABLE
        | attr
        | ap
}

unsafe fn install_translation_tables() {
    let mair = ATTR_INDEX_NORMAL | ATTR_INDEX_DEVICE | ATTR_INDEX_NORMAL_NC;
    // T0SZ=25 starts 4 KiB translation at level 1, matching the three tables below:
    // L1 -> L2 -> L3 pages for the low 2 MiB.
    let tcr = 25u64 | (1u64 << 8) | (1u64 << 10) | (3u64 << 12);
    asm!("msr mair_el2, {}", in(reg) mair, options(nostack, preserves_flags));
    asm!("msr tcr_el2, {}", in(reg) tcr, options(nostack, preserves_flags));
    asm!(
        "msr ttbr0_el2, {}",
        in(reg) core::ptr::addr_of!(L1_TABLE) as u64,
        options(nostack, preserves_flags)
    );
    asm!("dsb sy; isb", options(nostack, preserves_flags));

    let mut sctlr: u64;
    asm!("mrs {}, sctlr_el2", out(reg) sctlr, options(nostack, preserves_flags));
    sctlr |= 1;
    sctlr |= 1 << 2;
    sctlr |= 1 << 12;
    asm!("msr sctlr_el2, {}", in(reg) sctlr, options(nostack, preserves_flags));
    asm!("isb", options(nostack, preserves_flags));

    let _ = core::ptr::addr_of!(SHARD_MEMORY);
}

unsafe fn tlbi() {
    asm!(
        "dsb sy; tlbi alle2; dsb sy; isb",
        options(nostack, preserves_flags)
    );
}
