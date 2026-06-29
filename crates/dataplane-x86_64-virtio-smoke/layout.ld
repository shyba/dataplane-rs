ENTRY(_start)

MEMORY
{
  RAM : ORIGIN = 0x00010000, LENGTH = 512K
}

SECTIONS
{
  .text ORIGIN(RAM) : ALIGN(4096)
  {
    KEEP(*(.text._start))
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

  /DISCARD/ :
  {
    *(.comment)
    *(.note*)
    *(.eh_frame*)
  }
}

