use core::arch::asm;
use core::mem::size_of;
use core::ptr::{addr_of_mut, read_volatile, write_volatile};

use crate::serial;

#[allow(dead_code)]
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

static mut IDT: [IdtEntry; 256] = [IdtEntry::missing(); 256];
static mut DRIVER_FAULT_SEEN: bool = false;

#[allow(dead_code)]
#[no_mangle]
pub(crate) extern "C" fn page_fault_handler(frame: *mut u64) {
    // SAFETY: this handler runs on the hand-written exception frame installed by
    // the boot path. It only rewrites RIP for the known driver isolation probe.
    unsafe {
        let rip_slot = frame.add(1);
        let rip = read_volatile(rip_slot);
        let probe = (&driver_fault_probe as *const u8) as u64;
        if rip == probe {
            DRIVER_FAULT_SEEN = true;
            write_volatile(rip_slot, (&driver_fault_resume as *const u8) as u64);
            return;
        }
    }

    serial::write_str("DPX86:FAIL:unexpected-page-fault\n");
    halt_forever()
}

pub(crate) fn init_fault_isr() {
    // SAFETY: static IDT table and pointer are process-local and only initialized once at boot.
    unsafe {
        let handler = page_fault_entry as usize as u64;
        write_volatile(addr_of_mut!(IDT[14]), IdtEntry::new(handler, 0x18, 0x8e00));
        let ptr = IdtPointer {
            limit: (size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: (addr_of_mut!(IDT) as *const _) as u64,
        };
        asm!(
            "lidt [{}]",
            in(reg) &ptr,
            options(readonly, nostack, preserves_flags),
        );
    }
}

pub(crate) fn fail(reason: &str) -> ! {
    serial::write_str("DPX86:FAIL:");
    serial::write_str(reason);
    serial::write_str("\n");
    halt_forever()
}

pub(crate) fn halt_forever() -> ! {
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

pub(crate) fn driver_fault_seen() -> bool {
    unsafe { DRIVER_FAULT_SEEN }
}

extern "C" {
    fn page_fault_entry();
    #[allow(dead_code)]
    static driver_fault_probe: u8;
    #[allow(dead_code)]
    static driver_fault_resume: u8;
}
