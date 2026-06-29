#![no_std]
#![no_main]

mod arch;
mod block_runtime;
mod cli;
mod control_protocol;
mod fat32;
mod http;
mod kernel;
mod kernel_ledgers;
mod kernel_text;
mod layout;
mod net_protocol;
mod network_task;
mod scenarios;
mod serial;
mod services;
mod task_mailbox;
mod task_memory;
mod tcp_stream;
mod timer_model;
mod virtio_block;
mod virtio_net;

pub(crate) use arch::{
    align_up, block_region_is_mmu_covered, compiler_fence, fs_region_is_mmu_covered, inb, inw,
    net_region_is_mmu_covered, outb, outl, outw, pci, protected_block_region, protected_fs_region,
    protected_net_region, prove_fault_containment, prove_net_region_fault_containment,
};
pub(crate) use block_runtime::*;
pub(crate) use dataplane_microkernel_core::{
    CapabilityId, EndpointId, Message, MessageBody, NetworkFrameDescriptor, NetworkFrameDirection,
    NetworkFrameType, TaskId, TaskStatus,
};
pub(crate) use fat32::*;
pub(crate) use http::*;
pub(crate) use kernel::*;
pub(crate) use kernel_text::*;
pub(crate) use layout::*;
pub(crate) use network_task::*;
pub(crate) use task_mailbox::*;
pub(crate) use task_memory::*;
pub(crate) use tcp_stream::*;
pub(crate) use virtio_net::*;

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    kernel::rust_main()
}
