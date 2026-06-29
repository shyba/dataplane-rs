core::arch::global_asm!(
    r#"
    .section .text.boot, "ax"
    .global _start
_start:
    mrs x0, mpidr_el1
    and x0, x0, #3
    cbz x0, 2f
1:
    wfe
    b 1b
2:
    adr x0, vectors
    msr vbar_el3, x0
    mrs x1, CurrentEL
    cmp x1, #0xc
    b.ne 6f
    mov x1, #0x501
    msr scr_el3, x1
    mov x1, #0x3c9
    msr spsr_el3, x1
    adr x1, 6f
    msr elr_el3, x1
    eret
6:
    adr x0, vectors
    msr vbar_el2, x0
    msr vbar_el1, x0
    ldr x0, =__boot_stack_top
    mov sp, x0
    ldr x0, =__bss_start
    ldr x1, =__bss_end
3:
    cmp x0, x1
    b.hs 4f
    str xzr, [x0], #8
    b 3b
4:
    bl raspi3b_main
5:
    wfe
    b 5b

    .align 11
vectors:
    b current_el_sp0_sync
    .balign 128
    b current_el_sp0_irq
    .balign 128
    b current_el_sp0_fiq
    .balign 128
    b current_el_sp0_serror
    .balign 128
    b current_el_spx_sync
    .balign 128
    b current_el_spx_irq
    .balign 128
    b current_el_spx_fiq
    .balign 128
    b current_el_spx_serror
    .balign 128
    b lower_el_aarch64_sync
    .balign 128
    b lower_el_aarch64_irq
    .balign 128
    b lower_el_aarch64_fiq
    .balign 128
    b lower_el_aarch64_serror
    .balign 128
    b lower_el_aarch32_sync
    .balign 128
    b lower_el_aarch32_irq
    .balign 128
    b lower_el_aarch32_fiq
    .balign 128
    b lower_el_aarch32_serror

current_el_sp0_sync:
current_el_sp0_irq:
current_el_sp0_fiq:
current_el_sp0_serror:
current_el_spx_irq:
current_el_spx_fiq:
current_el_spx_serror:
lower_el_aarch64_sync:
lower_el_aarch64_irq:
lower_el_aarch64_fiq:
lower_el_aarch64_serror:
lower_el_aarch32_sync:
lower_el_aarch32_irq:
lower_el_aarch32_fiq:
lower_el_aarch32_serror:
    mov x0, #0x22
    b semihost_exit_asm

current_el_spx_sync:
    bl handle_sync_exception
    eret

semihost_exit_asm:
    mov x1, x0
    mov x0, #0x18
    hlt #0xf000
    b .
"#
);

#[no_mangle]
extern "C" fn raspi3b_main() -> ! {
    crate::orchestration::run()
}
