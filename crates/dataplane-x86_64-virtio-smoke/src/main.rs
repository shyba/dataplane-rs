#![no_std]
#![no_main]

use core::arch::global_asm;
use core::ptr::addr_of_mut;

use dataplane_runtime::noalloc::{drive_counts_step, FixedLocalExecCounts};

mod fault;
mod io;
mod mmu;
mod network;
mod pci;
mod serial;
#[cfg(feature = "shard-bench")]
mod shard_bus;
#[cfg(feature = "shard-bench")]
mod shard_bus_bench;
mod virtio;

const SERIAL_COM1: u16 = 0x3f8;

#[repr(align(4096))]
struct TaskRegion([u8; network::NET_TASK_BYTES]);

static mut NET_TASK_REGION: TaskRegion = TaskRegion([0; network::NET_TASK_BYTES]);

global_asm!(
    r#"
.section .text._start,"ax"
.global _start
_start:
    cld
    call rust_main
1:
    hlt
    jmp 1b

.section .text,"ax"
.global page_fault_entry
page_fault_entry:
    cld
    mov rdi, rsp
    call page_fault_handler
    add rsp, 8
    iretq

.global trigger_driver_fault
.global driver_fault_probe
.global driver_fault_resume
trigger_driver_fault:
driver_fault_probe:
    mov byte ptr [rdi], 0x5a
driver_fault_resume:
    ret
"#
);

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    serial::init();
    serial::write_str("DPX86:BOOT\n");

    unsafe {
        fault::init_fault_isr();
        mmu::install_identity_with_task_pages();
    }

    let mut scheduler = FixedLocalExecCounts::<1, 8>::new();
    if scheduler.push_count(0, 1).is_err() {
        fault::fail("scheduler");
    }

    let step = drive_counts_step(&mut scheduler, 1, 1, false);
    if step.progressed != 1 {
        fault::fail("scheduler-step");
    }

    #[cfg(feature = "shard-bench")]
    shard_bus_bench::run();

    let mut task = network::NetworkDriverTask::new(
        dataplane_microkernel_core::FixedNetworkTask::new(virtio::VirtioLegacyPciNet::new()),
    );
    let outcome = with_network_region(|region| task.run(region));
    match outcome {
        network::TaskOutcome::VmCommunication => {
            serial::write_str("DPX86:NET-TX\n");
            serial::write_str("DPX86:NET-RX\n");
            #[cfg(feature = "udp-bench")]
            serial::write_str("DPX86:UDP-BENCH\n");
        }
        network::TaskOutcome::Failed(reason) => fault::fail(reason.as_str()),
    }

    unsafe {
        with_network_region(|_| {});
        trigger_driver_fault(net_region_addr());
        if !fault::driver_fault_seen() {
            fault::fail("driver-fault-missing");
        }
    }

    serial::write_str("DPX86:DRIVER-FAULT-CONTAINED\n");
    serial::write_str("DPX86:OK\n");
    fault::halt_forever()
}

fn with_network_region<F, R>(f: F) -> R
where
    F: FnOnce(&mut network::NetworkTaskMemory<'_>) -> R,
{
    unsafe {
        let addr = net_region_addr() as *mut u8;
        mmu::allow_region(addr as usize, network::NET_TASK_BYTES);
        let region = addr_of_mut!((*addr_of_mut!(NET_TASK_REGION)).0);
        let mut memory = network::NetworkTaskMemory::new(&mut *region);
        let result = f(&mut memory);
        mmu::deny_region(addr as usize, network::NET_TASK_BYTES);
        result
    }
}

fn net_region_addr() -> *mut u8 {
    addr_of_mut!(NET_TASK_REGION) as *mut u8
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    fault::fail("panic")
}

extern "C" {
    fn trigger_driver_fault(addr: *mut u8);
}
