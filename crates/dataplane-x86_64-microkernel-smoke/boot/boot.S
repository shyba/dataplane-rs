.code16
.global _start

.equ STAGE2_SEG, 0x0800
.equ STAGE2_SECTORS, 16

_start:
    cli
    xor %ax, %ax
    mov %ax, %ds
    mov %ax, %ss
    mov $0x7c00, %sp
    mov %dl, boot_drive

    mov $STAGE2_SEG, %ax
    mov %ax, %es
    xor %bx, %bx
    mov boot_drive, %dl
    xor %ch, %ch
    xor %dh, %dh
    mov $2, %cl
    mov $(0x0200 | STAGE2_SECTORS), %ax
    int $0x13
    jc disk_error

    ljmp $STAGE2_SEG, $0

disk_error:
    mov $0x3f8, %dx
    mov $'B', %al
    out %al, %dx
    hlt
    jmp disk_error

boot_drive:
    .byte 0

.org 510
.word 0xaa55
