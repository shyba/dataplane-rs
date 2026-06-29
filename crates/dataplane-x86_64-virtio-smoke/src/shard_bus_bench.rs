use core::ptr::addr_of_mut;

use dataplane_microkernel_core::{
    CapabilityId, EndpointId, MailboxError, Message, MessageBody, RequestId, TaskId,
};
use dataplane_runtime::noalloc::FixedLocalExecCounts;

use crate::{fault, mmu, serial, shard_bus};

const MESSAGES: u64 = 100_000;
const BATCH_SIZES: [usize; 4] = [1, 8, 32, 128];
const CONSUMER_SLOT: usize = 1;
const MAX_PER_SLOT: usize = shard_bus::MAILBOX_CAP * 2;

#[repr(align(4096))]
struct CopyRegion([u64; 2]);

static mut COPY_SRC: CopyRegion = CopyRegion([0; 2]);
static mut COPY_DST: CopyRegion = CopyRegion([0; 2]);

pub(crate) fn run() {
    copy_baseline();
    for batch in BATCH_SIZES {
        queue_scenario(batch);
        rtt_scenario(batch);
    }
    fairness_scenario();
    service_scenario();
}

fn copy_baseline() {
    deny_copy_regions();
    let start = rdtsc();
    let mut seq = 0u64;
    while seq < MESSAGES {
        with_copy_src(|src| {
            src[0] = seq;
            src[1] = shard_bus::checksum(seq);
        });
        with_copy_dst(|dst| {
            dst[0] = seq;
            dst[1] = shard_bus::checksum(seq);
        });
        seq += 1;
    }
    let cycles = rdtsc().wrapping_sub(start);
    emit_perf("DPX86:SHARD-BENCH-COPY", 0, MESSAGES, cycles);
    serial::write_str("\n");

    with_copy_dst(|dst| {
        if dst[0] != MESSAGES - 1 || dst[1] != shard_bus::checksum(MESSAGES - 1) {
            fault::fail("shard-bench-copy");
        }
    });
    deny_copy_regions();
}

fn queue_scenario(batch: usize) {
    shard_bus::reset_regions();
    let mut ready = FixedLocalExecCounts::<2, MAX_PER_SLOT>::new();
    let mut next_seq = 0u64;
    let mut received = 0u64;
    let mut notify_count = 0u64;
    let start = rdtsc();

    while next_seq < MESSAGES {
        let produced = produce_batch(&mut next_seq, batch);
        if produced == 0 {
            fault::fail("shard-bus-produce");
        }
        notify(&mut ready, produced);
        notify_count += 1;
        received += drain_consumer(&mut ready, batch, received, false);
    }

    while ready.has_work() {
        received += drain_consumer(&mut ready, batch, received, false);
    }

    let cycles = rdtsc().wrapping_sub(start);
    if next_seq != MESSAGES || received != MESSAGES {
        fault::fail("shard-bus-queue-count");
    }

    let stats = shard_bus::with_bus(|bus| bus.consumer_stats());
    if stats.received != MESSAGES || stats.drops != 0 {
        fault::fail("shard-bus-queue-stats");
    }

    emit_queue(batch, cycles, notify_count, stats.full, stats.max_depth);
    serial::write_str("DPX86:SHARD-BUS-QUEUE endpoints=2 messages=");
    serial::write_u64(MESSAGES);
    serial::write_str(" drops=");
    serial::write_u64(stats.drops);
    serial::write_str("\n");
    shard_bus::deny_regions();
}

fn rtt_scenario(batch: usize) {
    shard_bus::reset_regions();
    let mut ready = FixedLocalExecCounts::<2, MAX_PER_SLOT>::new();
    let mut next_seq = 0u64;
    let mut received = 0u64;
    let mut notify_count = 0u64;
    let start = rdtsc();

    while next_seq < MESSAGES {
        let produced = produce_batch(&mut next_seq, batch);
        if produced == 0 {
            fault::fail("shard-bus-rtt-produce");
        }
        notify(&mut ready, produced);
        notify_count += 1;
        received += drain_consumer(&mut ready, batch, received, true);
    }

    while ready.has_work() {
        received += drain_consumer(&mut ready, batch, received, true);
    }

    let cycles = rdtsc().wrapping_sub(start);
    let (ack_count, ack_batches) =
        shard_bus::with_producer_shard(|producer| (producer.ack_count, producer.ack_batches));
    if next_seq != MESSAGES || received != MESSAGES || ack_count != MESSAGES {
        fault::fail("shard-bus-rtt-count");
    }

    let stats = shard_bus::with_bus(|bus| bus.consumer_stats());
    emit_rtt(batch, cycles, notify_count, ack_batches, ack_count);
    serial::write_str("DPX86:SHARD-BUS-ACK messages=");
    serial::write_u64(MESSAGES);
    serial::write_str(" ack_count=");
    serial::write_u64(ack_count);
    serial::write_str(" drops=");
    serial::write_u64(stats.drops);
    serial::write_str("\n");
    shard_bus::deny_regions();
}

fn service_scenario() {
    shard_bus::reset_regions();
    let message = shard_bus::network_status_message();
    match shard_bus::with_bus(|bus| bus.try_send(shard_bus::CONTROL_ENDPOINT, message)) {
        Ok(()) => {}
        Err(MailboxError::Full(_)) => fault::fail("shard-bus-service-send"),
        Err(MailboxError::Empty) => fault::fail("shard-bus-service-empty"),
    }

    let received = match shard_bus::with_bus(|bus| bus.drain_one(shard_bus::CONTROL_ENDPOINT)) {
        Ok(message) => message,
        Err(MailboxError::Empty) => fault::fail("shard-bus-service-missing"),
        Err(MailboxError::Full(_)) => fault::fail("shard-bus-service-full"),
    };
    if shard_bus::validate_network_status(received).is_err() {
        fault::fail("shard-bus-service-message");
    }

    let ack = shard_bus::control_ack_message();
    match shard_bus::with_bus(|bus| bus.try_send(shard_bus::NETWORK_ENDPOINT, ack)) {
        Ok(()) => {}
        Err(MailboxError::Full(_)) => fault::fail("shard-bus-service-ack-send"),
        Err(MailboxError::Empty) => fault::fail("shard-bus-service-ack-empty"),
    }
    let ack = match shard_bus::with_bus(|bus| bus.drain_one(shard_bus::NETWORK_ENDPOINT)) {
        Ok(message) => message,
        Err(MailboxError::Empty) => fault::fail("shard-bus-service-ack-missing"),
        Err(MailboxError::Full(_)) => fault::fail("shard-bus-service-ack-full"),
    };
    if shard_bus::validate_control_ack(ack).is_err() {
        fault::fail("shard-bus-service-ack-message");
    }

    let (control_stats, network_stats) =
        shard_bus::with_bus(|bus| (bus.control_stats(), bus.network_stats()));
    if control_stats.sent != 1
        || control_stats.received != 1
        || network_stats.sent != 1
        || network_stats.received != 1
        || control_stats.drops != 0
        || network_stats.drops != 0
    {
        fault::fail("shard-bus-service-stats");
    }

    serial::write_str(
        "DPX86:SHARD-BUS-SERVICE src=network dst=control requests=1 replies=1 drops=",
    );
    serial::write_u64(control_stats.drops + network_stats.drops);
    serial::write_str("\n");
    shard_bus::deny_regions();
}

fn fairness_scenario() {
    const PER_SESSION: u64 = 256;
    const TIMER_TICKS: u64 = 64;
    const SESSION_A_TASK: TaskId = TaskId::new(10);
    const SESSION_B_TASK: TaskId = TaskId::new(11);
    const SESSION_C_TASK: TaskId = TaskId::new(12);
    const FAIR_CAPABILITY: CapabilityId = CapabilityId::new(3);

    shard_bus::reset_regions();
    let mut ready_a = FixedLocalExecCounts::<1, MAX_PER_SLOT>::new();
    let mut ready_b = FixedLocalExecCounts::<1, MAX_PER_SLOT>::new();
    let mut ready_c = FixedLocalExecCounts::<1, MAX_PER_SLOT>::new();
    let mut session_a = FairSession::new(SESSION_A_TASK, shard_bus::CONSUMER_ENDPOINT);
    let mut session_b = FairSession::new(SESSION_B_TASK, shard_bus::CONTROL_ENDPOINT);
    let mut session_c = FairSession::new(SESSION_C_TASK, shard_bus::NETWORK_ENDPOINT);
    let mut timer_ticks = 0u64;
    let mut timer_gap = 0u64;
    let mut max_timer_gap = 0u64;

    while session_a.received < PER_SESSION
        || session_b.received < PER_SESSION
        || session_c.received < PER_SESSION
        || timer_ticks < TIMER_TICKS
    {
        produce_fair(&mut session_a, &mut ready_a, PER_SESSION, FAIR_CAPABILITY);
        produce_fair(&mut session_b, &mut ready_b, PER_SESSION, FAIR_CAPABILITY);
        produce_fair(&mut session_c, &mut ready_c, PER_SESSION, FAIR_CAPABILITY);

        drain_fair(&mut session_a, &mut ready_a, FAIR_CAPABILITY);
        drain_fair(&mut session_b, &mut ready_b, FAIR_CAPABILITY);
        drain_fair(&mut session_c, &mut ready_c, FAIR_CAPABILITY);

        timer_gap += 1;
        if timer_ticks < TIMER_TICKS {
            timer_ticks += 1;
            if timer_gap > max_timer_gap {
                max_timer_gap = timer_gap;
            }
            timer_gap = 0;
        }
    }

    let drops = shard_bus::with_bus(|bus| {
        bus.consumer_stats().drops + bus.control_stats().drops + bus.network_stats().drops
    });
    if session_a.received != PER_SESSION
        || session_b.received != PER_SESSION
        || session_c.received != PER_SESSION
        || timer_ticks != TIMER_TICKS
        || drops != 0
    {
        fault::fail("shard-bus-fairness");
    }

    let max_session_gap = max3(
        session_a.max_service_gap,
        session_b.max_service_gap,
        session_c.max_service_gap,
    );
    serial::write_str("DPX86:SHARD-FAIRNESS sessions=3 messages=");
    serial::write_u64(PER_SESSION * 3);
    serial::write_str(" timer_ticks=");
    serial::write_u64(timer_ticks);
    serial::write_str(" max_timer_gap=");
    serial::write_u64(max_timer_gap);
    serial::write_str(" max_session_gap=");
    serial::write_u64(max_session_gap);
    serial::write_str(" drops=");
    serial::write_u64(drops);
    serial::write_str("\n");
    shard_bus::deny_regions();
}

struct FairSession {
    task: TaskId,
    endpoint: EndpointId,
    produced: u64,
    received: u64,
    service_gap: u64,
    max_service_gap: u64,
}

impl FairSession {
    const fn new(task: TaskId, endpoint: EndpointId) -> Self {
        Self {
            task,
            endpoint,
            produced: 0,
            received: 0,
            service_gap: 0,
            max_service_gap: 0,
        }
    }
}

fn produce_fair(
    session: &mut FairSession,
    ready: &mut FixedLocalExecCounts<1, MAX_PER_SLOT>,
    limit: u64,
    capability: CapabilityId,
) {
    if session.produced >= limit {
        return;
    }
    let seq = session.produced;
    let message = Message::new(
        session.task,
        session.endpoint,
        RequestId::new(seq as u32),
        capability,
        MessageBody::Word(seq as u32),
    );
    match shard_bus::with_bus(|bus| bus.try_send(session.endpoint, message)) {
        Ok(()) => {}
        Err(MailboxError::Full(_)) => fault::fail("shard-bus-fair-send"),
        Err(MailboxError::Empty) => fault::fail("shard-bus-fair-empty"),
    }
    if ready.push_count(0, 1).is_err() {
        fault::fail("shard-bus-fair-notify");
    }
    session.produced += 1;
}

fn drain_fair(
    session: &mut FairSession,
    ready: &mut FixedLocalExecCounts<1, MAX_PER_SLOT>,
    capability: CapabilityId,
) {
    session.service_gap += 1;
    ready.drain(1, 1, false, || {
        let message = match shard_bus::with_bus(|bus| bus.drain_one(session.endpoint)) {
            Ok(message) => message,
            Err(MailboxError::Empty) => fault::fail("shard-bus-fair-missing"),
            Err(MailboxError::Full(_)) => fault::fail("shard-bus-fair-full"),
        };
        if message.from != session.task
            || message.to != session.endpoint
            || message.request != RequestId::new(session.received as u32)
            || message.capability != capability
            || message.body != MessageBody::Word(session.received as u32)
        {
            fault::fail("shard-bus-fair-message");
        }
        if session.service_gap > session.max_service_gap {
            session.max_service_gap = session.service_gap;
        }
        session.service_gap = 0;
        session.received += 1;
    });
}

fn max3(a: u64, b: u64, c: u64) -> u64 {
    let ab = if a > b { a } else { b };
    if ab > c {
        ab
    } else {
        c
    }
}

fn produce_batch(next_seq: &mut u64, limit: usize) -> usize {
    let mut produced = 0usize;
    while produced < limit && *next_seq < MESSAGES {
        let message = shard_bus::bench_message(*next_seq);
        let result = shard_bus::with_bus(|bus| bus.try_send(shard_bus::CONSUMER_ENDPOINT, message));
        match result {
            Ok(()) => {
                shard_bus::with_producer_shard(|producer| {
                    producer.produced += 1;
                });
                *next_seq += 1;
                produced += 1;
            }
            Err(MailboxError::Full(_)) => break,
            Err(MailboxError::Empty) => fault::fail("shard-bus-send-empty"),
        }
    }
    produced
}

fn notify(ready: &mut FixedLocalExecCounts<2, MAX_PER_SLOT>, count: usize) {
    if ready.push_count(CONSUMER_SLOT, count).is_err() {
        fault::fail("shard-bus-notify");
    }
}

fn drain_consumer(
    ready: &mut FixedLocalExecCounts<2, MAX_PER_SLOT>,
    budget: usize,
    expected_start: u64,
    ack: bool,
) -> u64 {
    let mut drained = 0u64;
    let mut last_seq = 0u64;
    ready.drain(1, budget, false, || {
        let message = match shard_bus::with_bus(|bus| bus.drain_one(shard_bus::CONSUMER_ENDPOINT)) {
            Ok(message) => message,
            Err(MailboxError::Empty) => fault::fail("shard-bus-empty-drain"),
            Err(MailboxError::Full(_)) => fault::fail("shard-bus-full-drain"),
        };
        let seq = match shard_bus::validate_bench_message(message) {
            Ok(seq) => seq,
            Err(()) => fault::fail("shard-bus-message"),
        };
        if seq != expected_start + drained {
            fault::fail("shard-bus-order");
        }
        last_seq = seq;
        drained += 1;
    });

    if drained != 0 && ack {
        shard_bus::with_producer_shard(|producer| {
            producer.ack_count += drained;
            producer.ack_batches += 1;
            producer.last_ack_seq = last_seq;
        });
    }
    drained
}

fn emit_queue(batch: usize, cycles: u64, notify_count: u64, full: u64, max_depth: usize) {
    emit_perf("DPX86:SHARD-BENCH-QUEUE", batch, MESSAGES, cycles);
    serial::write_str(" notify_count=");
    serial::write_u64(notify_count);
    serial::write_str(" queue_full_count=");
    serial::write_u64(full);
    serial::write_str(" max_depth=");
    serial::write_u64(max_depth as u64);
    serial::write_str("\n");
}

fn emit_rtt(batch: usize, cycles: u64, notify_count: u64, ack_batches: u64, ack_count: u64) {
    emit_perf("DPX86:SHARD-BENCH-RTT", batch, MESSAGES, cycles);
    serial::write_str(" notify_count=");
    serial::write_u64(notify_count);
    serial::write_str(" ack_batches=");
    serial::write_u64(ack_batches);
    serial::write_str(" ack_count=");
    serial::write_u64(ack_count);
    serial::write_str("\n");
}

fn emit_perf(marker: &str, batch: usize, messages: u64, cycles: u64) {
    let cycles_per_msg = cycles / messages;
    let msgs_per_mcycle = if cycles == 0 {
        0
    } else {
        messages.saturating_mul(1_000_000) / cycles
    };
    serial::write_str(marker);
    if batch != 0 {
        serial::write_str(" batch=");
        serial::write_u64(batch as u64);
    }
    serial::write_str(" messages=");
    serial::write_u64(messages);
    serial::write_str(" bytes_per_msg=");
    serial::write_u64(shard_bus::BENCH_PAYLOAD_BYTES as u64);
    serial::write_str(" cycles=");
    serial::write_u64(cycles);
    serial::write_str(" cycles_per_msg=");
    serial::write_u64(cycles_per_msg);
    serial::write_str(" msgs_per_mcycle=");
    serial::write_u64(msgs_per_mcycle);
}

fn with_copy_src<R>(f: impl FnOnce(&mut [u64; 2]) -> R) -> R {
    with_copy_region(addr_of_mut!(COPY_SRC), f)
}

fn with_copy_dst<R>(f: impl FnOnce(&mut [u64; 2]) -> R) -> R {
    with_copy_region(addr_of_mut!(COPY_DST), f)
}

fn with_copy_region<R>(addr: *mut CopyRegion, f: impl FnOnce(&mut [u64; 2]) -> R) -> R {
    unsafe {
        mmu::allow_region(addr as usize, core::mem::size_of::<CopyRegion>());
        let result = f(&mut (*addr).0);
        mmu::deny_region(addr as usize, core::mem::size_of::<CopyRegion>());
        result
    }
}

fn deny_copy_regions() {
    unsafe {
        mmu::deny_region(
            addr_of_mut!(COPY_SRC) as usize,
            core::mem::size_of::<CopyRegion>(),
        );
        mmu::deny_region(
            addr_of_mut!(COPY_DST) as usize,
            core::mem::size_of::<CopyRegion>(),
        );
    }
}

#[inline(always)]
fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "lfence",
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
    }
    (u64::from(hi) << 32) | u64::from(lo)
}
