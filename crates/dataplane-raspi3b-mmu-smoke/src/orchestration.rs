use dataplane_microkernel_core::FixedNetworkTask;
use dataplane_runtime::noalloc::FixedLocalExecCounts;

use crate::config::{
    ProtectedShard, ShardId, DATA_SHARD_COUNT, ETHERNET_REGION, FORBIDDEN_SHARD, REGION_COUNT,
    STEPS_PER_REGION,
};
use crate::ethernet::UsbEthernetTask;
use crate::mmu;
use crate::semihost::semihost_exit;
use crate::vm_uart::VmUartTask;

pub(crate) fn run() -> ! {
    mmu::init();

    let shards = [
        ProtectedShard { id: ShardId(0) },
        ProtectedShard { id: ShardId(1) },
        ProtectedShard { id: ShardId(2) },
        ProtectedShard {
            id: ShardId(ETHERNET_REGION),
        },
    ];
    let mut ethernet = FixedNetworkTask::new(UsbEthernetTask::new());
    let mut ethernet_tx_ready = false;
    let mut vm_uart = VmUartTask::new();

    let mut work: FixedLocalExecCounts<REGION_COUNT, STEPS_PER_REGION> =
        FixedLocalExecCounts::new();
    let mut region = 0usize;
    while region < REGION_COUNT {
        work.push_count(region, STEPS_PER_REGION).unwrap();
        region += 1;
    }

    while work.has_work() {
        let mut progressed = [0usize; REGION_COUNT];
        let step_count = work.drain(REGION_COUNT, 1, false, || {});
        assert_eq!(step_count, REGION_COUNT);

        let mut index = 0usize;
        while index < DATA_SHARD_COUNT {
            shards[index].with_access(|bytes| {
                bytes[0] = bytes[0].wrapping_add(1);
                bytes[1] = index as u8;
            });
            progressed[index] += 1;
            index += 1;
        }

        shards[ETHERNET_REGION].with_access(|bytes| {
            let _ = ethernet.init(bytes);
            let _ = ethernet.transmit_frame(bytes, crate::ethernet::rndis_probe_frame());
            if ethernet.driver_mut().frame_sent() {
                ethernet_tx_ready = true;
            }
            let _ = ethernet.receive_frame(bytes);
            vm_uart.poll(bytes);
        });
        progressed[ETHERNET_REGION] += 1;

        assert_eq!(progressed, [1, 1, 1, 1]);
    }

    if !ethernet_tx_ready {
        semihost_exit(0x45);
    }
    if !vm_uart.communicated() {
        semihost_exit(0x44);
    }

    mmu::trigger_forbidden_access(shards[FORBIDDEN_SHARD]);

    if mmu::seen_fault() {
        semihost_exit(0);
    }
    semihost_exit(1);
}
