use core::arch::asm;
use core::panic::PanicInfo;

pub(crate) fn semihost_exit(code: u32) -> ! {
    let args = [0x20026u64, code as u64];
    unsafe {
        asm!(
            "mov x0, #0x20",
            "hlt #0xf000",
            in("x1") args.as_ptr(),
            options(noreturn)
        );
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    semihost_exit(0x2);
}
