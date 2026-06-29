ENTRY(_start)

MEMORY
{
  RAM : ORIGIN = 0x00080000, LENGTH = 16M
}

SECTIONS
{
  .text ORIGIN(RAM) : ALIGN(4096)
  {
    KEEP(*(.text.boot))
    *(.text .text.*)
  } > RAM

  .rodata : ALIGN(4096)
  {
    *(.rodata .rodata.*)
  } > RAM

  .data : ALIGN(4096)
  {
    *(.data .data.*)
  } > RAM

  .bss : ALIGN(4096)
  {
    __bss_start = .;
    *(.bss .bss.*)
    *(COMMON)
    __bss_end = .;
  } > RAM

  . = ALIGN(4096);
  __boot_stack_bottom = .;
  . += 0x4000;
  __boot_stack_top = .;

  /DISCARD/ :
  {
    *(.comment)
    *(.note*)
    *(.eh_frame*)
  }
}
