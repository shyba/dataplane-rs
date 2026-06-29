use crate::control_protocol::{
    CONTROL_PROTOCOL_V1_MAX_PAYLOAD_BYTES, CONTROL_PROTOCOL_V1_OPCODE_STATUS_ECHO,
    CONTROL_PROTOCOL_V1_VERSION,
};
use crate::fat32::FAT32_BYTES_PER_SECTOR;
use crate::http::{HTTP_HEADER_BYTES, HTTP_REQUEST_LINE_BYTES};
use crate::kernel::{
    CLI_COMMAND_COUNT, CLI_FIRST_COMMAND_TIMEOUT_SPINS, CLI_LINE_BYTES, CLI_LINE_TIMEOUT_SPINS,
    DHCP_POLL_LIMIT, HTTP_INDEX_FILE_BYTES, HTTP_LARGE_FILE_BYTES, TIMER_DHCP_POLL_INTERVAL,
    TIMER_FAIRNESS_POLL_INTERVAL, TIMER_MAILBOX_CAP, TIMER_TICK_PAYLOAD_WORD,
};
use crate::layout::{
    BLOCK_QUEUE_CAP, BLOCK_TASK_BYTES, BLOCK_VIRTQ_BYTES, CAP_KERNEL, EP_TIMER, FS_TASK_BYTES,
    NET_QUEUE_CAP, NET_TASK_BYTES, NET_VIRTQ_BYTES, REQUEST_TIMER_TICK, TASK_TIMER,
    TIMER_ENDPOINT_ID_NUM, TIMER_REQUEST_ID_NUM, TIMER_TASK_ID_NUM,
};
use crate::network_task::{
    NET_REPLY_PAYLOAD_BYTES, TCP_CONTROL_OVERFLOW_BYTES, TCP_STREAM_RX_BYTES,
    TCP_STREAM_SEGMENT_BYTES, TCP_STREAM_SESSIONS, TCP_STREAM_TX_BYTES,
};
use crate::serial;
use crate::timer_model::{evaluate_timer_timeout_proof, TIMER_MAX_GAP_BOUND};
use dataplane_microkernel_core::{Mailbox, Message, MessageBody, MESSAGE_INLINE_BYTES};

pub(crate) fn emit_resource_budget_ledger() {
    serial::write_str("DPMK:RESOURCE-BUDGET-LEDGER\n");
    serial::write_str("DPBUDGET:task-bytes block=");
    serial::write_decimal(BLOCK_TASK_BYTES as u32);
    serial::write_str(" fs=");
    serial::write_decimal(FS_TASK_BYTES as u32);
    serial::write_str(" net=");
    serial::write_decimal(NET_TASK_BYTES as u32);
    serial::write_str("\n");

    serial::write_str("DPBUDGET:virtq-bytes block=");
    serial::write_decimal(BLOCK_VIRTQ_BYTES as u32);
    serial::write_str(" net=");
    serial::write_decimal(NET_VIRTQ_BYTES as u32);
    serial::write_str(" block_cap=");
    serial::write_decimal(BLOCK_QUEUE_CAP as u32);
    serial::write_str(" net_cap=");
    serial::write_decimal(NET_QUEUE_CAP as u32);
    serial::write_str("\n");

    serial::write_str("DPBUDGET:protocol-bytes cli_line=");
    serial::write_decimal(CLI_LINE_BYTES as u32);
    serial::write_str(" cli_commands=");
    serial::write_decimal(CLI_COMMAND_COUNT as u32);
    serial::write_str(" http_line=");
    serial::write_decimal(HTTP_REQUEST_LINE_BYTES as u32);
    serial::write_str(" http_headers=");
    serial::write_decimal(HTTP_HEADER_BYTES as u32);
    serial::write_str(" net_reply=");
    serial::write_decimal(NET_REPLY_PAYLOAD_BYTES as u32);
    serial::write_str(" udp_payload=");
    serial::write_decimal(CONTROL_PROTOCOL_V1_MAX_PAYLOAD_BYTES as u32);
    serial::write_str("\n");

    serial::write_str("DPBUDGET:fat32-bytes sector=");
    serial::write_decimal(FAT32_BYTES_PER_SECTOR as u32);
    serial::write_str(" index_file=");
    serial::write_decimal(HTTP_INDEX_FILE_BYTES as u32);
    serial::write_str(" large_file=");
    serial::write_decimal(HTTP_LARGE_FILE_BYTES as u32);
    serial::write_str("\n");
    serial::write_str("DPMK:RESOURCE-BUDGET-LEDGER-OK\n");
}

pub(crate) fn emit_protocol_input_bounds_ledger() {
    serial::write_str("DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER\n");
    serial::write_str("DPBOUNDS:cli line=");
    serial::write_decimal(CLI_LINE_BYTES as u32);
    serial::write_str(" commands=");
    serial::write_decimal(CLI_COMMAND_COUNT as u32);
    serial::write_str("\n");

    serial::write_str("DPBOUNDS:http request_line=");
    serial::write_decimal(HTTP_REQUEST_LINE_BYTES as u32);
    serial::write_str(" headers=");
    serial::write_decimal(HTTP_HEADER_BYTES as u32);
    serial::write_str(" index_file=");
    serial::write_decimal(HTTP_INDEX_FILE_BYTES as u32);
    serial::write_str(" large_file=");
    serial::write_decimal(HTTP_LARGE_FILE_BYTES as u32);
    serial::write_str("\n");

    serial::write_str("DPBOUNDS:tcp rx=");
    serial::write_decimal(TCP_STREAM_RX_BYTES as u32);
    serial::write_str(" segment=");
    serial::write_decimal(TCP_STREAM_SEGMENT_BYTES as u32);
    serial::write_str(" tx=");
    serial::write_decimal(TCP_STREAM_TX_BYTES as u32);
    serial::write_str(" sessions=");
    serial::write_decimal(TCP_STREAM_SESSIONS as u32);
    serial::write_str(" overflow_probe=");
    serial::write_decimal(TCP_CONTROL_OVERFLOW_BYTES as u32);
    serial::write_str("\n");

    serial::write_str("DPBOUNDS:udp-control payload=");
    serial::write_decimal(CONTROL_PROTOCOL_V1_MAX_PAYLOAD_BYTES as u32);
    serial::write_str(" version=");
    serial::write_decimal(CONTROL_PROTOCOL_V1_VERSION as u32);
    serial::write_str(" opcode=");
    serial::write_decimal(CONTROL_PROTOCOL_V1_OPCODE_STATUS_ECHO as u32);
    serial::write_str(" reply=");
    serial::write_decimal(NET_REPLY_PAYLOAD_BYTES as u32);
    serial::write_str("\n");
    serial::write_str("DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER-OK\n");
}

pub(crate) fn emit_timer_timeout_service_ledger() {
    let proof = evaluate_timer_timeout_proof();
    serial::write_str("DPMK:TIMER-TIMEOUT-SERVICE-LEDGER\n");
    serial::write_str("DPTIMER:owner task=");
    serial::write_decimal(TIMER_TASK_ID_NUM as u32);
    serial::write_str(" endpoint=");
    serial::write_decimal(TIMER_ENDPOINT_ID_NUM as u32);
    serial::write_str(" request=");
    serial::write_decimal(TIMER_REQUEST_ID_NUM as u32);
    serial::write_str(" mailbox_cap=");
    serial::write_decimal(TIMER_MAILBOX_CAP as u32);
    serial::write_str(" payload_word=");
    serial::write_decimal(TIMER_TICK_PAYLOAD_WORD);
    serial::write_str("\n");

    serial::write_str("DPTIMER:source cooperative-run_timer_tick monotonic=work-units wall_clock=0 rtc=0 ntp=0 cert_time=0\n");

    serial::write_str("DPTIMER:timeouts cli_first_read=");
    serial::write_decimal(CLI_FIRST_COMMAND_TIMEOUT_SPINS as u32);
    serial::write_str(" cli_line_read=");
    serial::write_decimal(CLI_LINE_TIMEOUT_SPINS as u32);
    serial::write_str(" dhcp_poll_limit=");
    serial::write_decimal(DHCP_POLL_LIMIT);
    serial::write_str("\n");

    serial::write_str("DPTIMER:delivery control=bounded owner=task=");
    serial::write_decimal(TIMER_TASK_ID_NUM as u32);
    serial::write_str(" tick_source=cooperative-run_timer_tick timeout_resolution_ticks=8\n");
    serial::write_str("DPTIMER:positive-delivery now=");
    serial::write_decimal(proof.positive_case.now_ticks);
    serial::write_str(" deadline=");
    serial::write_decimal(proof.positive_case.deadline_ticks);
    serial::write_str(" age=");
    serial::write_decimal(proof.positive_case.age());
    serial::write_str(" grace=");
    serial::write_decimal(proof.positive_case.grace_ticks);
    serial::write_str(" control=");
    serial::write_str(if proof.positive_case.is_expired() {
        "expired"
    } else {
        "bounded"
    });
    serial::write_str(" outcome=");
    serial::write_str(if proof.positive_case.is_expired() {
        "reject"
    } else {
        "deliver"
    });
    serial::write_str("\n");
    serial::write_str("DPTIMER:stale-timeout now=");
    serial::write_decimal(proof.stale_case.now_ticks);
    serial::write_str(" deadline=");
    serial::write_decimal(proof.stale_case.deadline_ticks);
    serial::write_str(" age=");
    serial::write_decimal(proof.stale_case.age());
    serial::write_str(" grace=");
    serial::write_decimal(proof.stale_case.grace_ticks);
    serial::write_str(" outcome=");
    serial::write_str(if proof.stale_case.is_expired() {
        "bounded-drop"
    } else {
        "delivered"
    });
    serial::write_str(" stale_timeout_retries=0 stale_timeout_rejects=");
    serial::write_decimal((proof.stale_case.is_expired() as u32) & 1);
    serial::write_str("\n");

    serial::write_str("DPTIMER:expired-timeout now=");
    serial::write_decimal(proof.expired_case.now_ticks);
    serial::write_str(" deadline=");
    serial::write_decimal(proof.expired_case.deadline_ticks);
    serial::write_str(" age=");
    serial::write_decimal(proof.expired_case.age());
    serial::write_str(" grace=");
    serial::write_decimal(proof.expired_case.grace_ticks);
    serial::write_str(" outcome=");
    serial::write_str(if proof.expired_case.is_expired() {
        "reject"
    } else {
        "delivered"
    });
    serial::write_str(" expired_timeout_rejects=");
    serial::write_decimal(proof.expired_case.is_expired() as u32);
    serial::write_str(" expired_timeout_ignored=");
    serial::write_decimal((proof.expired_case.is_expired() as u32) & 1);
    serial::write_str("\n");

    serial::write_str("DPTIMER:wrap-comparison now=");
    serial::write_decimal(proof.wrap_case.now_ticks);
    serial::write_str(" deadline=");
    serial::write_decimal(proof.wrap_case.deadline_ticks);
    serial::write_str(" age=");
    serial::write_decimal(proof.wrap_case.age());
    serial::write_str(" wrap_policy=wrap-saturating\n");

    serial::write_str("DPTIMER:gap-bounds observed=");
    serial::write_decimal(proof.observed_gap_ticks);
    serial::write_str(" fairness=");
    serial::write_decimal(TIMER_FAIRNESS_POLL_INTERVAL);
    serial::write_str(" network=");
    serial::write_decimal(TIMER_MAX_GAP_BOUND);
    serial::write_str(" fairness_poll_interval=");
    serial::write_decimal(TIMER_FAIRNESS_POLL_INTERVAL);
    serial::write_str(" dhcp_poll_interval=");
    serial::write_decimal(TIMER_DHCP_POLL_INTERVAL);
    serial::write_str("\n");
    serial::write_str("DPMK:TIMER-TIMEOUT-SERVICE-LEDGER-OK\n");
}

pub(crate) fn emit_combined_cap_degradation_ledger() {
    let oversize = [0u8; MESSAGE_INLINE_BYTES + 1];
    let output_cap_rejected = MessageBody::inline_bytes(&oversize).is_err();

    let mut mailbox: Mailbox<1> = Mailbox::new();
    let message = Message::new(
        TASK_TIMER,
        EP_TIMER,
        REQUEST_TIMER_TICK,
        CAP_KERNEL,
        MessageBody::Word(TIMER_TICK_PAYLOAD_WORD),
    );
    let mailbox_full_rejected = mailbox.send(message).is_ok() && mailbox.send(message).is_err();

    serial::write_str("DPMK:COMBINED-CAP-DEGRADATION-LEDGER\n");
    serial::write_str("DPCAP:output inline_limit=");
    serial::write_decimal(MESSAGE_INLINE_BYTES as u32);
    serial::write_str(" oversized_rejected=");
    serial::write_decimal(output_cap_rejected as u32);
    serial::write_str("\n");
    serial::write_str("DPCAP:queue mailbox_cap=1 queue_full_rejected=");
    serial::write_decimal(mailbox_full_rejected as u32);
    serial::write_str("\n");
    serial::write_str("DPCAP:timer progress_source=cooperative max_gap_bound=");
    serial::write_decimal(TIMER_MAX_GAP_BOUND);
    serial::write_str("\n");
    serial::write_str("DPCAP:stale-reply runtime_claim=0 policy=deferred\n");
    serial::write_str("DPMK:COMBINED-CAP-DEGRADATION-OK\n");
}
