//! Exercise pure production modules without executing port I/O or MMU code.
#![allow(dead_code)]

#[path = "../src/fat32.rs"]
mod fat32;
#[path = "../src/layout.rs"]
mod layout;
#[path = "../src/net_protocol.rs"]
mod net_protocol;
#[path = "../src/task_memory.rs"]
mod task_memory;

use layout::*;

#[repr(align(4096))]
struct NetRegion([u8; NET_TASK_BYTES]);

#[test]
fn queue_word_accesses_preserve_little_endian_values() {
    let mut region = NetRegion([0; NET_TASK_BYTES]);
    task_memory::write_u16_net_region(&mut region.0, 2, 0xabcd);
    assert_eq!(&region.0[2..4], &[0xcd, 0xab]);
    assert_eq!(task_memory::read_u16_net_region(&region.0, 2), 0xabcd);
    region.0[4..8].copy_from_slice(&0x12345678u32.to_le_bytes());
    assert_eq!(task_memory::read_u32_net_region(&region.0, 4), 0x12345678);
}

#[test]
fn receive_copy_excludes_transport_header_and_rejects_oversize() {
    let mut bytes = [0; NET_TASK_BYTES];
    let payload = RX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN;
    bytes[payload..payload + 3].copy_from_slice(b"abc");
    let mut memory = task_memory::NetTaskMemory { bytes: &mut bytes };
    assert!(memory
        .copy_virtio_rx_frame((2048 - VIRTIO_NET_HDR_LEN + 1) as u32)
        .is_err());
    memory.copy_virtio_rx_frame(3).unwrap();
    assert_eq!(memory.frame(3).unwrap(), b"abc");
    assert!(memory.frame(NET_TASK_BYTES - NET_FRAME_OFFSET + 1).is_err());
}

#[test]
fn fixed_filesystem_rejects_invalid_boot_and_forged_handle() {
    use fat32::*;
    let mut bytes = [0; FS_TASK_BYTES];
    let memory = FsTaskMemory { bytes: &mut bytes };
    let task = FsTask::new();
    assert!(task.parse_boot_sector(&memory).is_err());
    assert!(FsKnownPath::from_path_bytes(b"/../HELLO.TXT").is_none());
    let handle = FsFileHandle {
        path: FsKnownPath::Hello,
        cluster: FAT32_HELLO_CLUSTER,
        size: FAT32_HELLO_CONTENT.len() as u32,
    };
    assert!(task.validate_handle(handle).is_ok());
    assert!(task
        .validate_handle(FsFileHandle {
            cluster: 0,
            ..handle
        })
        .is_err());
    assert!(task
        .validate_handle(FsFileHandle {
            size: u32::MAX,
            ..handle
        })
        .is_err());
}

#[test]
fn ipv4_header_checksum_round_trips_and_detects_corruption() {
    use net_protocol::*;
    let mut header = [0; IPV4_HEADER_BYTES];
    write_ipv4_header(&mut header, 17, 8, 42, [10, 0, 0, 1], [10, 0, 0, 2]);
    assert_eq!(read_be_u16(&header, 2), 28);
    assert_eq!(checksum_finish(checksum_add_bytes(0, &header)), 0);
    header[8] ^= 1;
    assert_ne!(checksum_finish(checksum_add_bytes(0, &header)), 0);
}
