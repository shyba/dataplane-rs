use core::arch::asm;
use core::ptr::addr_of_mut;

#[repr(align(4096))]
struct PageTable([u64; 512]);

const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const HUGE: u64 = 1 << 7;
const PAGE: usize = 4096;

static mut PML4: PageTable = PageTable([0; 512]);
static mut PDPT: PageTable = PageTable([0; 512]);
static mut PD0: PageTable = PageTable([0; 512]);
static mut PD1: PageTable = PageTable([0; 512]);
static mut PD2: PageTable = PageTable([0; 512]);
static mut PD3: PageTable = PageTable([0; 512]);
static mut PT0: PageTable = PageTable([0; 512]);

pub unsafe fn install_identity_with_task_pages() {
    let pml4 = addr_of_mut!(PML4);
    let pdpt = addr_of_mut!(PDPT);
    let pd0 = addr_of_mut!(PD0);
    let pd1 = addr_of_mut!(PD1);
    let pd2 = addr_of_mut!(PD2);
    let pd3 = addr_of_mut!(PD3);
    let pt0 = addr_of_mut!(PT0);

    clear(pml4);
    clear(pdpt);
    clear(pd0);
    clear(pd1);
    clear(pd2);
    clear(pd3);
    clear(pt0);

    (*pml4).0[0] = table(pdpt);
    (*pdpt).0[0] = table(pd0);
    (*pdpt).0[1] = table(pd1);
    (*pdpt).0[2] = table(pd2);
    (*pdpt).0[3] = table(pd3);
    (*pd0).0[0] = table(pt0);

    for index in 0..512 {
        (*pt0).0[index] = (index as u64 * PAGE as u64) | PRESENT | WRITABLE;
    }

    fill_huge(pd0, 1, 0);
    fill_huge(pd1, 0, 0x4000_0000);
    fill_huge(pd2, 0, 0x8000_0000);
    fill_huge(pd3, 0, 0xc000_0000);

    let cr3 = core::ptr::addr_of!(PML4) as u64;
    asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags));
}

pub unsafe fn allow_region(addr: usize, len: usize) {
    set_region(addr, len, true);
}

pub unsafe fn deny_region(addr: usize, len: usize) {
    set_region(addr, len, false);
}

unsafe fn set_region(addr: usize, len: usize, present: bool) {
    let mut page = addr & !(PAGE - 1);
    let end = (addr + len + PAGE - 1) & !(PAGE - 1);
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
