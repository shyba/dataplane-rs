.code16
.global stage2_start

.equ KERNEL_SEG, 0x1000
.ifndef KERNEL_SECTORS
.equ KERNEL_SECTORS, 512
.endif
.equ KERNEL_LBA, 17
.equ PML4_ADDR, 0x70000
.equ PDPT_ADDR, 0x71000
.equ PD0_ADDR, 0x72000
.equ PD1_ADDR, 0x73000
.equ PD2_ADDR, 0x74000
.equ PD3_ADDR, 0x75000
.equ STACK_TOP, 0x90000
.equ KERNEL_ENTRY, 0x10000

stage2_start:
    cli
    mov %dl, boot_drive
    xor %ax, %ax
    mov %ax, %ds
    mov %ax, %ss
    mov $0x7c00, %sp

    in $0x92, %al
    or $0x02, %al
    out %al, $0x92

    mov $KERNEL_SEG, %bp
    mov $KERNEL_LBA, %di
    mov $KERNEL_SECTORS, %si

load_kernel:
    mov %bp, %ax
    mov %ax, %es
    xor %bx, %bx
    push %si
    push %di
    push %bp
    call read_lba_sector
    pop %bp
    pop %di
    pop %si
    inc %di
    add $0x20, %bp
    dec %si
    jnz load_kernel

    lgdt gdt32_ptr
    mov %cr0, %eax
    or $0x1, %eax
    mov %eax, %cr0
    ljmp $0x08, $protected_start

read_lba_sector:
    push %ax
    push %bx
    push %cx
    push %dx
    push %di

    mov %di, %ax
    xor %dx, %dx
    mov $18, %cx
    div %cx
    inc %dl
    mov %dl, %bl

    xor %dx, %dx
    mov $2, %cx
    div %cx
    mov %al, %ch
    mov %bl, %cl
    mov %dl, %dh
    mov boot_drive, %dl
    xor %bx, %bx
    mov $0x0201, %ax
    int $0x13
    jc disk_error

    pop %di
    pop %dx
    pop %cx
    pop %bx
    pop %ax
    ret

disk_error:
    mov $0x3f8, %dx
    mov $'S', %al
    out %al, %dx
    hlt
    jmp disk_error

.code32
protected_start:
    mov $0x10, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss
    mov %ax, %fs
    mov %ax, %gs
    mov $STACK_TOP, %esp

    cld
    mov $PML4_ADDR, %edi
    xor %eax, %eax
    mov $(0x6000 / 4), %ecx
    rep stosl

    mov $(PDPT_ADDR | 0x003), %eax
    mov %eax, PML4_ADDR

    mov $(PD0_ADDR | 0x003), %eax
    mov %eax, PDPT_ADDR
    mov $(PD1_ADDR | 0x003), %eax
    mov %eax, PDPT_ADDR + 8
    mov $(PD2_ADDR | 0x003), %eax
    mov %eax, PDPT_ADDR + 16
    mov $(PD3_ADDR | 0x003), %eax
    mov %eax, PDPT_ADDR + 24

    mov $PD0_ADDR, %edi
    xor %ebx, %ebx
    call fill_pd
    mov $PD1_ADDR, %edi
    mov $0x40000000, %ebx
    call fill_pd
    mov $PD2_ADDR, %edi
    mov $0x80000000, %ebx
    call fill_pd
    mov $PD3_ADDR, %edi
    mov $0xc0000000, %ebx
    call fill_pd

    mov %cr4, %eax
    or $(1 << 5), %eax
    mov %eax, %cr4

    mov $PML4_ADDR, %eax
    mov %eax, %cr3

    mov $0xc0000080, %ecx
    rdmsr
    or $(1 << 8), %eax
    wrmsr

    mov %cr0, %eax
    or $0x80000000, %eax
    mov %eax, %cr0

    lgdt gdt64_ptr
    ljmp $0x18, $long_mode_start

fill_pd:
    mov $512, %ecx
fill_pd_loop:
    mov %ebx, %eax
    or $0x083, %eax
    mov %eax, (%edi)
    movl $0, 4(%edi)
    add $0x200000, %ebx
    add $8, %edi
    loop fill_pd_loop
    ret

.code64
long_mode_start:
    mov $0x20, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss
    mov $STACK_TOP, %rsp
    xor %rbp, %rbp
    call *kernel_entry_ptr(%rip)
halt_forever:
    hlt
    jmp halt_forever

.align 8
kernel_entry_ptr:
    .quad KERNEL_ENTRY

.align 8
gdt32:
    .quad 0
    .quad 0x00cf9a000000ffff
    .quad 0x00cf92000000ffff
gdt32_end:
gdt32_ptr:
    .word gdt32_end - gdt32 - 1
    .long gdt32

.align 8
gdt64:
    .quad 0
    .quad 0
    .quad 0
    .quad 0x00af9a000000ffff
    .quad 0x00af92000000ffff
gdt64_end:
gdt64_ptr:
    .word gdt64_end - gdt64 - 1
    .long gdt64

boot_drive:
    .byte 0
