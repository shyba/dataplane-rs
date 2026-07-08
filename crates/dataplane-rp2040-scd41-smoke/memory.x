MEMORY {
  BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
  /* The first 512 KiB are firmware. Rolling-log storage starts at 0x10080000. */
  FLASH : ORIGIN = 0x10000100, LENGTH = 512K - 0x100
  RAM   : ORIGIN = 0x20000000, LENGTH = 256K
}
