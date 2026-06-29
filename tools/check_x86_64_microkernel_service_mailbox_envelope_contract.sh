#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

core="crates/dataplane-microkernel-core/src/lib.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
task_mailbox="crates/dataplane-x86_64-microkernel-smoke/src/task_mailbox.rs"
main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
layout="crates/dataplane-x86_64-microkernel-smoke/src/layout.rs"
services="crates/dataplane-x86_64-microkernel-smoke/src/services.rs"
makefile="Makefile"
runner="tools/x86_64_microkernel_fat32_run.sh"
plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"
diary="aidocs/050_microkernel_robust_design_diary_2026-05-30.md"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_literal() {
  local file="$1"
  local literal="$2"
  local note="$3"
  grep -Fq -- "$literal" "$file" || fail "$note"
}

require_literal_any() {
  local literal="$1"
  local note="$2"
  shift 2
  local file
  for file in "$@"; do
    if grep -Fq -- "$literal" "$file"; then
      return 0
    fi
  done
  fail "$note"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local out="/tmp/dataplane-mailbox-contract.$$"
  local err="/tmp/dataplane-mailbox-contract-err.$$"
  local status
  set +e
  rg -n -- "$regex" "$file" >"$out" 2>"$err"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat "$out"
    rm -f "$out" "$err"
    fail "$note"
  fi
  if [[ "$status" -ne 1 ]]; then
    cat "$err" >&2 || true
    rm -f "$out" "$err"
    fail "could not scan $file for forbidden regex: $regex"
  fi
  rm -f "$out" "$err"
}

for file in "$core" "$main" "$layout" "$services" "$makefile" "$runner" "$plan" "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Service Mailbox Envelope Contract ==="

for literal in \
  '#![no_std]' \
  '#![forbid(unsafe_code)]' \
  'pub const MESSAGE_INLINE_BYTES: usize = 32;' \
  'pub enum MessageBody' \
  'InlineBytes {' \
  'pub struct Message {' \
  'pub enum MailboxError {' \
  'Full(Message),' \
  'Empty,' \
  'pub struct Mailbox<const N: usize>' \
  'slots: [Option<Message>; N]' \
  'pub fn enqueue(&mut self, message: Message) -> Result<(), MailboxError>' \
  'pub fn dequeue(&mut self) -> Result<Message, MailboxError>'; do
  require_literal "$core" "$literal" "core crate must preserve no-alloc mailbox/envelope literal: $literal"
done

for literal in \
  'emit_service_mailbox_envelope_ledger();' \
  'Mailbox<SERVICE_MAILBOX_CAP>' \
  'KernelState {'; do
  require_literal "$kernel" "$literal" "kernel source must preserve service mailbox literal: $literal"
done

for literal in \
  'Mailbox<SERVICE_MAILBOX_CAP>' \
  'mailbox.send(message).map_err(|_| "mailbox-send")?' \
  'mailbox.recv().map_err(|_| "mailbox-recv")?'; do
  require_literal "$task_mailbox" "$literal" "task mailbox source must preserve service mailbox literal: $literal"
done

for literal in \
  'TaskTable,' \
  'Mailbox<SERVICE_MAILBOX_CAP>'; do
  require_literal "$kernel" "$literal" "guest source must preserve service mailbox literal: $literal"
done

for literal in \
  'pub const MESSAGE_INLINE_BYTES: usize = 32;'; do
  require_literal "$core" "$literal" "core source must preserve service mailbox literal: $literal"
done

for literal in \
  'const SERVICE_MAILBOX_CAP: usize = 4;' \
  'pub(crate) fn emit_service_mailbox_envelope_ledger()' \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER' \
  'DPMBOX:shape fields=from,to,request,capability,body inline_bytes=' \
  'DPMBOX:tasks timer=' \
  'DPMBOX:requests timer_tick=' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred' \
  'DPMBOX:fault-destination task=' \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-OK' \
  '.record_fault(TASK_BLOCK)'; do
  require_literal "$services" "$literal" "guest source must preserve service mailbox literal: $literal"
done

for literal in \
  'pub(crate) const REQUEST_TIMER_TICK: RequestId = RequestId::new(TIMER_REQUEST_ID_NUM);' \
  'pub(crate) const REQUEST_CLI_FS: RequestId = RequestId::new(2);' \
  'pub(crate) const REQUEST_FS_BLOCK: RequestId = RequestId::new(3);' \
  'pub(crate) const REQUEST_TCPIP_NET: RequestId = RequestId::new(4);' \
  'pub(crate) const REQUEST_HTTP_FS: RequestId = RequestId::new(5);' \
  'pub(crate) const REQUEST_DHCP_NET: RequestId = RequestId::new(6);'; do
  require_literal_any "$literal" "guest source must preserve service mailbox literal: $literal" "$main" "$layout"
done

for literal in \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER' \
  'DPMBOX:shape fields=from' \
  'DPMBOX:tasks timer=1 cli=2 fs=3 block=4 net=5 tcpip=6 http=7 dhcp=8' \
  'DPMBOX:requests timer_tick=1 cli_fs=2 fs_block=3 tcpip_net=4 http_fs=5 dhcp_net=6' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred' \
  'DPMBOX:fault-destination task=4 status=faulted restart=0 replay=0' \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-OK'; do
  require_literal "$runner" "$literal" "QEMU runner must validate service mailbox evidence: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-service-mailbox-envelope-contract:' \
  "Makefile must expose service mailbox envelope contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_service_mailbox_envelope_contract.sh' \
  "Makefile must run service mailbox guard"
require_literal "$makefile" 'x86_64-microkernel-service-mailbox-envelope:' \
  "Makefile must expose service mailbox proof target"
require_literal "$plan" 'x86_64-microkernel-service-mailbox-envelope' \
  "plan must keep service mailbox envelope packet"
require_literal "$diary" 'x86_64-microkernel-service-mailbox-envelope' \
  "diary must record service mailbox envelope packet"

reject_regex "$services" 'DPMBOX.*(generic_actor=1|dyn_dispatch=1|heap=1|restart=1|replay=1|TLS|HTTPS|production IPC)' \
  "mailbox envelope ledger must not claim generic actor, heap, restart, replay, or TLS behavior"
reject_regex "$makefile" 'x86_64-microkernel-service-mailbox-envelope.*(strict-five|STRICT_FIVE_CALIBRATION|retune|calibration)' \
  "service mailbox target must not substitute benchmark calibration evidence"

echo "x86_64 microkernel service mailbox envelope contract OK"
