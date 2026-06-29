use core::arch::asm;

use crate::mmu;
use crate::semihost::semihost_exit;

#[no_mangle]
extern "C" fn handle_sync_exception() {
    let esr: u64;
    let elr: u64;
    unsafe {
        asm!("mrs {}, esr_el2", out(reg) esr, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, elr_el2", out(reg) elr, options(nomem, nostack, preserves_flags));
    }

    let ec = (esr >> 26) & 0x3f;
    if ec == 0x24 || ec == 0x25 {
        mmu::mark_fault(esr, elr);
        unsafe {
            asm!(
                "msr elr_el2, {}",
                in(reg) elr.wrapping_add(4),
                options(nomem, nostack, preserves_flags)
            );
        }
    } else {
        semihost_exit(0x30);
    }
}
