use std::net::Ipv6Addr;

use rustler::{Atom, Binary, Encoder, Env, LocalPid, NewBinary, OwnedBinary, ResourceArc, Term};

use crate::errors::NifError;
use crate::runtime;
use crate::runtime_protocol::BatchOp;
use crate::runtime_reactor::{local_addr_of_fd, peer_addr_of_fd};
use crate::socket::{ActiveMode, NetAddr, PacketMode, SocketKind, SocketRef};
use dataplane_nif::bench_support::{
    bench_queue_worker, bench_worker, BenchMsg, BenchOp, BenchQueueBatchOp, BenchQueueRequest,
    SessionBatchOp,
};
use dataplane_nif::error_terms::encode_error;
use std::sync::atomic::Ordering;

pub(crate) use dataplane_nif::atoms;

#[rustler::nif]
pub fn init(env: Env) -> Term {
    match runtime::start_runtime() {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn stop_runtime(env: Env) -> Term {
    match runtime::stop_runtime() {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn bench_direct_ok(env: Env) -> Term {
    atoms::ok().encode(env)
}

#[rustler::nif]
pub fn bench_direct_binary<'a>(env: Env<'a>, size: usize) -> Term<'a> {
    let mut bin = match rustler::OwnedBinary::new(size) {
        Some(bin) => bin,
        None => return (atoms::error(), atoms::error()).encode(env),
    };
    for byte in bin.as_mut_slice() {
        *byte = 0x5a;
    }
    (atoms::ok(), Binary::from_owned(bin, env)).encode(env)
}

#[rustler::nif]
pub fn bench_zero_read<'a>(env: Env<'a>, size: usize) -> Term<'a> {
    let mut bin = NewBinary::new(env, size);
    bin.as_mut_slice().fill(0);
    let out: Binary<'_> = bin.into();
    (atoms::ok(), out).encode(env)
}

#[rustler::nif]
pub fn bench_env_send_ok(env: Env, pid: LocalPid) -> Term {
    match env.send(&pid, atoms::bench_reply()) {
        Ok(()) => atoms::ok().encode(env),
        Err(_) => (atoms::error(), atoms::closed()).encode(env),
    }
}

#[rustler::nif]
pub fn bench_env_send_binary<'a>(env: Env<'a>, pid: LocalPid, size: usize) -> Term<'a> {
    let mut bin = match rustler::OwnedBinary::new(size) {
        Some(bin) => bin,
        None => return (atoms::error(), atoms::error()).encode(env),
    };
    for byte in bin.as_mut_slice() {
        *byte = 0x5a;
    }
    match env.send(
        &pid,
        (atoms::bench_reply_bin(), Binary::from_owned(bin, env)),
    ) {
        Ok(()) => atoms::ok().encode(env),
        Err(_) => (atoms::error(), atoms::closed()).encode(env),
    }
}

#[rustler::nif]
pub fn bench_zero_env_send_binary<'a>(env: Env<'a>, pid: LocalPid, size: usize) -> Term<'a> {
    let mut bin = NewBinary::new(env, size);
    bin.as_mut_slice().fill(0);
    let out: Binary<'_> = bin.into();
    match env.send(&pid, (atoms::bench_reply_bin(), out)) {
        Ok(()) => atoms::ok().encode(env),
        Err(_) => (atoms::error(), atoms::closed()).encode(env),
    }
}

#[rustler::nif]
pub fn bench_owned_env_send_ok(env: Env, pid: LocalPid) -> Term {
    let Some(worker) = bench_worker() else {
        return (atoms::error(), atoms::closed()).encode(env);
    };
    match worker.send(BenchMsg::Ok(pid)) {
        Ok(()) => atoms::ok().encode(env),
        Err(_) => (atoms::error(), atoms::closed()).encode(env),
    }
}

#[rustler::nif]
pub fn bench_owned_env_send_binary(env: Env, pid: LocalPid, size: usize) -> Term {
    let Some(worker) = bench_worker() else {
        return (atoms::error(), atoms::closed()).encode(env);
    };
    match worker.send(BenchMsg::Binary(pid, size)) {
        Ok(()) => atoms::ok().encode(env),
        Err(_) => (atoms::error(), atoms::closed()).encode(env),
    }
}

#[rustler::nif]
pub fn bench_zero_owned_env_send_binary(env: Env, pid: LocalPid, size: usize) -> Term {
    let Some(worker) = bench_worker() else {
        return (atoms::error(), atoms::closed()).encode(env);
    };
    match worker.send(BenchMsg::ZeroBinary(pid, size)) {
        Ok(()) => atoms::ok().encode(env),
        Err(_) => (atoms::error(), atoms::closed()).encode(env),
    }
}

#[rustler::nif]
pub fn bench_zero_owned_env_send_many(env: Env, pid: LocalPid, size: usize, count: usize) -> Term {
    let Some(worker) = bench_worker() else {
        return (atoms::error(), atoms::closed()).encode(env);
    };
    match worker.send(BenchMsg::ZeroMany(pid, size, count)) {
        Ok(()) => atoms::ok().encode(env),
        Err(_) => (atoms::error(), atoms::closed()).encode(env),
    }
}

#[rustler::nif]
pub fn bench_zero_owned_env_send_batch(env: Env, pid: LocalPid, size: usize, count: usize) -> Term {
    let Some(worker) = bench_worker() else {
        return (atoms::error(), atoms::closed()).encode(env);
    };
    match worker.send(BenchMsg::ZeroBatch(pid, size, count)) {
        Ok(()) => atoms::ok().encode(env),
        Err(_) => (atoms::error(), atoms::closed()).encode(env),
    }
}

#[rustler::nif]
pub fn bench_batch<'a>(env: Env<'a>, commands: Vec<Term<'a>>) -> Term<'a> {
    let mut replies = Vec::with_capacity(commands.len());
    for cmd in commands {
        let (id, op, payload): (u64, BenchOp, Term<'a>) = match cmd.decode() {
            Ok(decoded) => decoded,
            Err(_) => return (atoms::error(), atoms::einval()).encode(env),
        };
        match op {
            BenchOp::Write => {
                if payload.decode::<Binary>().is_err() {
                    return (atoms::error(), atoms::einval()).encode(env);
                }
                replies.push((id, atoms::ok(), atoms::undefined()).encode(env));
            }
            BenchOp::Read => {
                let size = match payload.decode::<usize>() {
                    Ok(size) => size,
                    Err(_) => return (atoms::error(), atoms::einval()).encode(env),
                };
                let mut bin = match OwnedBinary::new(size) {
                    Some(bin) => bin,
                    None => return (atoms::error(), atoms::error()).encode(env),
                };
                for byte in bin.as_mut_slice() {
                    *byte = 0x5a;
                }
                replies.push((id, atoms::ok(), Binary::from_owned(bin, env)).encode(env));
            }
        }
    }
    replies.encode(env)
}

#[rustler::nif]
pub fn bench_queue_batch_async<'a>(env: Env<'a>, commands: Vec<Term<'a>>) -> Term<'a> {
    let Some(worker) = bench_queue_worker() else {
        return (atoms::error(), atoms::closed()).encode(env);
    };
    let request_id = worker.next_request_id.fetch_add(1, Ordering::AcqRel);
    let mut ops = Vec::with_capacity(commands.len());
    for cmd in commands {
        let (id, op, payload): (u64, BenchOp, Term<'a>) = match cmd.decode() {
            Ok(decoded) => decoded,
            Err(_) => return (atoms::error(), atoms::einval()).encode(env),
        };
        match op {
            BenchOp::Write => {
                let data = match payload.decode::<Binary>() {
                    Ok(bin) => bin.as_slice().to_vec(),
                    Err(_) => return (atoms::error(), atoms::einval()).encode(env),
                };
                ops.push(BenchQueueBatchOp::Write { id, data });
            }
            BenchOp::Read => {
                let size = match payload.decode::<usize>() {
                    Ok(size0) => size0,
                    Err(_) => return (atoms::error(), atoms::einval()).encode(env),
                };
                ops.push(BenchQueueBatchOp::Read { id, size });
            }
        }
    }
    let request = BenchQueueRequest {
        request_id,
        reply_pid: env.pid(),
        ops,
    };
    worker.pending.fetch_add(1, Ordering::AcqRel);
    if worker.tx.send(request).is_err() {
        worker.pending.fetch_sub(1, Ordering::AcqRel);
        return (atoms::error(), atoms::closed()).encode(env);
    }
    (atoms::ok(), request_id).encode(env)
}

#[rustler::nif]
pub fn bench_runtime_nop_async(env: Env) -> Term {
    match runtime::bench_runtime_nop_async(env.pid()) {
        Ok(request_id) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn caller_shard(env: Env) -> Term {
    match runtime::caller_shard() {
        Ok(shard) => (atoms::ok(), shard as u64).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn stat_async(env: Env, fd: i32) -> Term {
    match runtime::stat_async_to(fd, env.pid()) {
        Ok(request_id) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn stat_async_to(env: Env, fd: i32, reply_pid: LocalPid) -> Term {
    match runtime::stat_async_to(fd, reply_pid) {
        Ok(request_id) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn batch_async<'a>(
    env: Env<'a>,
    sock_ref: ResourceArc<SocketRef>,
    commands: Vec<Term<'a>>,
) -> Term<'a> {
    let SocketKind::Session(session) = &sock_ref.kind else {
        return (atoms::error(), atoms::einval()).encode(env);
    };
    if session.is_closed() {
        return (atoms::error(), atoms::closed()).encode(env);
    }

    let request_id = match runtime::alloc_request_id(&sock_ref) {
        Ok(id0) => id0,
        Err(e) => return err_atom(env, &e),
    };
    let mut ops = Vec::with_capacity(commands.len());
    for cmd in commands {
        let (id, op, payload): (u64, SessionBatchOp, Term<'a>) = match cmd.decode() {
            Ok(decoded) => decoded,
            Err(_) => return (atoms::error(), atoms::einval()).encode(env),
        };
        match op {
            SessionBatchOp::Write => {
                let data = match payload.decode::<Binary>() {
                    Ok(bin) => bin.as_slice().to_vec(),
                    Err(_) => return (atoms::error(), atoms::einval()).encode(env),
                };
                ops.push(BatchOp::Write { id, data });
            }
            SessionBatchOp::Read => {
                let len = match payload.decode::<usize>() {
                    Ok(len0) => len0,
                    Err(_) => return (atoms::error(), atoms::einval()).encode(env),
                };
                ops.push(BatchOp::Read { id, len });
            }
        }
    }
    match runtime::batch_async(sock_ref, request_id, env.pid(), ops) {
        Ok(()) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn command<'a>(env: Env<'a>, cmd: Term<'a>) -> Term<'a> {
    if let Ok((verb, pid, op_atom, fd)) = cmd.decode::<(Atom, LocalPid, Atom, i32)>() {
        if verb != atoms::subscribe() {
            return (atoms::error(), atoms::einval()).encode(env);
        }
        let op = if op_atom == atoms::read() {
            runtime::SubscribeOperation::Read
        } else if op_atom == atoms::accept() {
            runtime::SubscribeOperation::Accept
        } else {
            return (atoms::error(), atoms::einval()).encode(env);
        };
        return match runtime::subscribe(pid, op, fd) {
            Ok(subscription_id) => (atoms::ok(), subscription_id).encode(env),
            Err(e) => err_atom(env, &e),
        };
    }

    if let Ok((verb, subcmd, subscription_id)) = cmd.decode::<(Atom, Atom, u64)>() {
        if verb == atoms::subscribe() && subcmd == atoms::new_consumer() {
            return match runtime::subscribe_add_consumer(env.pid(), subscription_id) {
                Ok(()) => atoms::ok().encode(env),
                Err(e) => err_atom(env, &e),
            };
        }
        return (atoms::error(), atoms::einval()).encode(env);
    }

    if let Ok((verb, subscription_id)) = cmd.decode::<(Atom, u64)>() {
        let control = if verb == atoms::choke() {
            runtime::SubscribeControl::Choke
        } else if verb == atoms::unchoke() {
            runtime::SubscribeControl::Unchoke
        } else if verb == atoms::stop() {
            runtime::SubscribeControl::Stop
        } else {
            return (atoms::error(), atoms::einval()).encode(env);
        };
        return match runtime::subscribe_control(subscription_id, control) {
            Ok(()) => atoms::ok().encode(env),
            Err(e) => err_atom(env, &e),
        };
    }

    (atoms::error(), atoms::einval()).encode(env)
}

#[rustler::nif]
pub fn debug_runtime_stats<'a>(env: Env<'a>) -> Term<'a> {
    match runtime::debug_runtime_stats() {
        Ok(stats) => {
            let items: Vec<Vec<(Atom, Term<'a>)>> = stats
                .into_iter()
                .map(|s| {
                    vec![
                        (atoms::shard(), s.shard.encode(env)),
                        (atoms::commands_drained(), s.commands_drained.encode(env)),
                        (atoms::ready_driven(), s.ready_driven.encode(env)),
                        (
                            atoms::write_ready_enqueued(),
                            s.write_ready_enqueued.encode(env),
                        ),
                        (
                            atoms::write_ready_drained(),
                            s.write_ready_drained.encode(env),
                        ),
                        (atoms::latency_cqes(), s.latency_cqes.encode(env)),
                        (atoms::main_cqes(), s.main_cqes.encode(env)),
                        (atoms::recv_sqes(), s.recv_sqes.encode(env)),
                        (atoms::recv_cqes(), s.recv_cqes.encode(env)),
                        (atoms::recv_wouldblock(), s.recv_wouldblock.encode(env)),
                        (atoms::recv_bytes(), s.recv_bytes.encode(env)),
                        (
                            atoms::read_poll_from_recv_eagain(),
                            s.read_poll_from_recv_eagain.encode(env),
                        ),
                        (
                            atoms::recv_wait_completed(),
                            s.recv_wait_completed.encode(env),
                        ),
                        (
                            atoms::recv_wait_completed_error(),
                            s.recv_wait_completed_error.encode(env),
                        ),
                        (
                            atoms::recv_wait_canceled(),
                            s.recv_wait_canceled.encode(env),
                        ),
                        (
                            atoms::recv_wait_timeout_canceled(),
                            s.recv_wait_timeout_canceled.encode(env),
                        ),
                        (
                            atoms::recv_wait_ns_total(),
                            s.recv_wait_ns_total.encode(env),
                        ),
                        (atoms::recv_wait_ns_max(), s.recv_wait_ns_max.encode(env)),
                        (atoms::recv_wait_le_1us(), s.recv_wait_le_1us.encode(env)),
                        (atoms::recv_wait_le_5us(), s.recv_wait_le_5us.encode(env)),
                        (atoms::recv_wait_le_20us(), s.recv_wait_le_20us.encode(env)),
                        (
                            atoms::recv_wait_le_100us(),
                            s.recv_wait_le_100us.encode(env),
                        ),
                        (
                            atoms::recv_wait_le_500us(),
                            s.recv_wait_le_500us.encode(env),
                        ),
                        (atoms::recv_wait_le_1ms(), s.recv_wait_le_1ms.encode(env)),
                        (atoms::recv_wait_le_5ms(), s.recv_wait_le_5ms.encode(env)),
                        (atoms::recv_wait_gt_5ms(), s.recv_wait_gt_5ms.encode(env)),
                        (
                            atoms::recv_ingress_wait_count(),
                            s.recv_ingress_wait_count.encode(env),
                        ),
                        (
                            atoms::recv_ingress_wait_ns_total(),
                            s.recv_ingress_wait_ns_total.encode(env),
                        ),
                        (
                            atoms::recv_ingress_wait_ns_max(),
                            s.recv_ingress_wait_ns_max.encode(env),
                        ),
                        (
                            atoms::recv_sync_ingress_wait_count(),
                            s.recv_sync_ingress_wait_count.encode(env),
                        ),
                        (
                            atoms::recv_sync_ingress_wait_ns_total(),
                            s.recv_sync_ingress_wait_ns_total.encode(env),
                        ),
                        (
                            atoms::recv_sync_ingress_wait_ns_max(),
                            s.recv_sync_ingress_wait_ns_max.encode(env),
                        ),
                        (
                            atoms::send_sync_ingress_wait_count(),
                            s.send_sync_ingress_wait_count.encode(env),
                        ),
                        (
                            atoms::send_sync_ingress_wait_ns_total(),
                            s.send_sync_ingress_wait_ns_total.encode(env),
                        ),
                        (
                            atoms::send_sync_ingress_wait_ns_max(),
                            s.send_sync_ingress_wait_ns_max.encode(env),
                        ),
                        (
                            atoms::listener_accept_wait_count(),
                            s.listener_accept_wait_count.encode(env),
                        ),
                        (
                            atoms::listener_accept_wait_ns_total(),
                            s.listener_accept_wait_ns_total.encode(env),
                        ),
                        (
                            atoms::listener_accept_wait_ns_max(),
                            s.listener_accept_wait_ns_max.encode(env),
                        ),
                        (
                            atoms::subscribe_accept_wait_count(),
                            s.subscribe_accept_wait_count.encode(env),
                        ),
                        (
                            atoms::subscribe_accept_wait_ns_total(),
                            s.subscribe_accept_wait_ns_total.encode(env),
                        ),
                        (
                            atoms::subscribe_accept_wait_ns_max(),
                            s.subscribe_accept_wait_ns_max.encode(env),
                        ),
                        (
                            atoms::recv_buf_append_count(),
                            s.recv_buf_append_count.encode(env),
                        ),
                        (
                            atoms::recv_buf_append_bytes(),
                            s.recv_buf_append_bytes.encode(env),
                        ),
                        (
                            atoms::recv_buf_tail_append_count(),
                            s.recv_buf_tail_append_count.encode(env),
                        ),
                        (
                            atoms::recv_buf_tail_append_bytes(),
                            s.recv_buf_tail_append_bytes.encode(env),
                        ),
                        (atoms::write_sqes(), s.write_sqes.encode(env)),
                        (atoms::write_cqes(), s.write_cqes.encode(env)),
                        (atoms::read_polls(), s.read_polls.encode(env)),
                        (atoms::write_polls(), s.write_polls.encode(env)),
                        (atoms::cancel_recv_cqes(), s.cancel_recv_cqes.encode(env)),
                        (
                            atoms::cancel_listener_accept_cqes(),
                            s.cancel_listener_accept_cqes.encode(env),
                        ),
                        (atoms::close_fd_cqes(), s.close_fd_cqes.encode(env)),
                        (atoms::reprovide_sqes(), s.reprovide_sqes.encode(env)),
                        (atoms::rx_queue_peak(), s.rx_queue_peak.encode(env)),
                        (atoms::tx_queue_peak(), s.tx_queue_peak.encode(env)),
                        (atoms::conn_count(), s.conn_count.encode(env)),
                        (atoms::ready_depth(), s.ready_depth.encode(env)),
                        (atoms::write_ready_depth(), s.write_ready_depth.encode(env)),
                        (
                            atoms::result_events_enqueued(),
                            s.result_events_enqueued.encode(env),
                        ),
                        (
                            atoms::result_reduce_runs_size(),
                            s.result_reduce_runs_size.encode(env),
                        ),
                        (
                            atoms::result_reduce_runs_age(),
                            s.result_reduce_runs_age.encode(env),
                        ),
                        (
                            atoms::result_reduce_runs_idle(),
                            s.result_reduce_runs_idle.encode(env),
                        ),
                        (
                            atoms::result_events_reduced(),
                            s.result_events_reduced.encode(env),
                        ),
                        (
                            atoms::send_tasks_enqueued(),
                            s.send_tasks_enqueued.encode(env),
                        ),
                        (atoms::send_batches_sent(), s.send_batches_sent.encode(env)),
                        (atoms::send_items_sent(), s.send_items_sent.encode(env)),
                        (atoms::send_failures(), s.send_failures.encode(env)),
                        (
                            atoms::result_send_wait_count(),
                            s.result_send_wait_count.encode(env),
                        ),
                        (
                            atoms::result_send_wait_ns_total(),
                            s.result_send_wait_ns_total.encode(env),
                        ),
                        (
                            atoms::result_send_wait_ns_max(),
                            s.result_send_wait_ns_max.encode(env),
                        ),
                        (
                            atoms::callback_fifo_peak(),
                            s.callback_fifo_peak.encode(env),
                        ),
                        (
                            atoms::callback_fifo_spill_count(),
                            s.callback_fifo_spill_count.encode(env),
                        ),
                    ]
                })
                .collect();
            (atoms::ok(), items).encode(env)
        }
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn listen(env: Env, port: u16, backlog: i32) -> Term {
    match runtime::listen(env.pid(), port, backlog) {
        Ok(listener) => (atoms::ok(), listener).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn accept(env: Env, listen_ref: ResourceArc<SocketRef>, timeout_ms: i64) -> Term {
    match runtime::accept(listen_ref, env.pid(), timeout_ms) {
        Ok(session) => (atoms::ok(), session).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn accept_async(env: Env, listen_ref: ResourceArc<SocketRef>, timeout_ms: i64) -> Term {
    match runtime::accept_async(listen_ref, env.pid(), timeout_ms, env.pid()) {
        Ok(request_id) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn accept_async_to(
    env: Env,
    listen_ref: ResourceArc<SocketRef>,
    timeout_ms: i64,
    reply_pid: LocalPid,
) -> Term {
    match runtime::accept_async(listen_ref, env.pid(), timeout_ms, reply_pid) {
        Ok(request_id) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn recv_async(env: Env, sock_ref: ResourceArc<SocketRef>, length: usize) -> Term {
    match runtime::recv_async(sock_ref, length, env.pid()) {
        Ok(request_id) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn recv_async_to(
    env: Env,
    sock_ref: ResourceArc<SocketRef>,
    length: usize,
    reply_pid: LocalPid,
) -> Term {
    match runtime::recv_async(sock_ref, length, reply_pid) {
        Ok(request_id) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif(schedule = "DirtyIo")]
pub fn recv(env: Env, sock_ref: ResourceArc<SocketRef>, length: usize, timeout_ms: i64) -> Term {
    match runtime::recv_sync(sock_ref, length, timeout_ms) {
        Ok(data) => {
            let mut bin = match rustler::OwnedBinary::new(data.len()) {
                Some(bin) => bin,
                None => return (atoms::error(), atoms::error()).encode(env),
            };
            bin.as_mut_slice().copy_from_slice(&data);
            (atoms::ok(), Binary::from_owned(bin, env)).encode(env)
        }
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn cancel_recv(env: Env, sock_ref: ResourceArc<SocketRef>) -> Term {
    match runtime::cancel_recv(sock_ref) {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn send<'a>(env: Env<'a>, sock_ref: ResourceArc<SocketRef>, data: Binary<'a>) -> Term<'a> {
    match runtime::send_sync(sock_ref, data.as_slice().to_vec()) {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn send_enqueue<'a>(
    env: Env<'a>,
    sock_ref: ResourceArc<SocketRef>,
    data: Binary<'a>,
) -> Term<'a> {
    match runtime::send_enqueue(sock_ref, data.as_slice().to_vec()) {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn send_enqueues<'a>(
    env: Env<'a>,
    sock_ref: ResourceArc<SocketRef>,
    packets: Vec<Binary<'a>>,
) -> Term<'a> {
    let data: Vec<Vec<u8>> = packets
        .into_iter()
        .map(|bin| bin.as_slice().to_vec())
        .collect();
    match runtime::send_many_enqueue(sock_ref, data) {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn send_async<'a>(
    env: Env<'a>,
    sock_ref: ResourceArc<SocketRef>,
    data: Binary<'a>,
) -> Term<'a> {
    match runtime::send_async(sock_ref, data.as_slice().to_vec(), env.pid()) {
        Ok(request_id) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn set_active_async<'a>(
    env: Env<'a>,
    sock_ref: ResourceArc<SocketRef>,
    mode: Term<'a>,
) -> Term<'a> {
    set_active_impl(env, sock_ref, mode, false)
}

#[rustler::nif]
pub fn set_active_enqueue<'a>(
    env: Env<'a>,
    sock_ref: ResourceArc<SocketRef>,
    mode: Term<'a>,
) -> Term<'a> {
    set_active_impl(env, sock_ref, mode, true)
}

fn set_active_impl<'a>(
    env: Env<'a>,
    sock_ref: ResourceArc<SocketRef>,
    mode: Term<'a>,
    immediate: bool,
) -> Term<'a> {
    let new_mode = if mode == atoms::true_().encode(env) {
        ActiveMode::True
    } else if mode == atoms::false_().encode(env) {
        ActiveMode::False
    } else if mode == atoms::once().encode(env) {
        ActiveMode::Once
    } else if let Ok(b) = mode.decode::<bool>() {
        if b {
            ActiveMode::True
        } else {
            ActiveMode::False
        }
    } else if let Ok(n) = mode.decode::<i32>() {
        if n > 0 {
            ActiveMode::N(n)
        } else {
            return (atoms::error(), atoms::einval()).encode(env);
        }
    } else {
        return (atoms::error(), atoms::einval()).encode(env);
    };

    if immediate {
        match runtime::set_active_enqueue(sock_ref, new_mode) {
            Ok(()) => atoms::ok().encode(env),
            Err(e) => err_atom(env, &e),
        }
    } else {
        match runtime::set_active_async(sock_ref, new_mode, env.pid()) {
            Ok(request_id) => (atoms::ok(), request_id).encode(env),
            Err(e) => err_atom(env, &e),
        }
    }
}

#[rustler::nif]
pub fn set_nodelay_async(env: Env, sock_ref: ResourceArc<SocketRef>, enabled: bool) -> Term {
    match runtime::set_nodelay_async(sock_ref, enabled, env.pid()) {
        Ok(request_id) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn set_nodelay_enqueue(env: Env, sock_ref: ResourceArc<SocketRef>, enabled: bool) -> Term {
    let _ = env;
    match runtime::set_nodelay_enqueue(sock_ref, enabled) {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn set_mailbox_passive_enqueue(
    env: Env,
    sock_ref: ResourceArc<SocketRef>,
    enabled: bool,
) -> Term {
    let _ = env;
    match runtime::set_mailbox_passive_enqueue(sock_ref, enabled) {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif(schedule = "DirtyIo")]
pub fn sendfile(
    env: Env,
    sock_ref: ResourceArc<SocketRef>,
    path: String,
    offset: u64,
    length: u64,
) -> Term {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return err_atom(env, &NifError::last_os_error()),
    };

    if offset > 0 && file.seek(SeekFrom::Start(offset)).is_err() {
        return err_atom(env, &NifError::last_os_error());
    }

    const SENDFILE_BATCH: usize = 8;

    let mut sent = 0u64;
    let mut remaining = length;
    let mut buf = vec![0u8; 64 * 1024];
    let mut queued = Vec::with_capacity(SENDFILE_BATCH);

    loop {
        let read_len = if remaining == 0 {
            buf.len()
        } else {
            remaining.min(buf.len() as u64) as usize
        };
        match file.read(&mut buf[..read_len]) {
            Ok(0) => break,
            Ok(n) => {
                queued.push(buf[..n].to_vec());
                sent += n as u64;
                if remaining > 0 {
                    remaining -= n as u64;
                }
                if queued.len() >= SENDFILE_BATCH || remaining == 0 {
                    match runtime::send_many_enqueue(sock_ref.clone(), std::mem::take(&mut queued))
                    {
                        Ok(()) => {}
                        Err(e) => return err_atom(env, &e),
                    }
                }
                if remaining == 0 {
                    break;
                }
            }
            Err(_) => return err_atom(env, &NifError::last_os_error()),
        }
    }

    if !queued.is_empty() {
        match runtime::send_many_enqueue(sock_ref, queued) {
            Ok(()) => {}
            Err(e) => return err_atom(env, &e),
        }
    }

    (atoms::ok(), sent).encode(env)
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn close(env: Env, sock_ref: ResourceArc<SocketRef>) -> Term {
    match runtime::close(sock_ref) {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn close_async(env: Env, sock_ref: ResourceArc<SocketRef>) -> Term {
    match runtime::close_async(sock_ref, env.pid()) {
        Ok(request_id) => (atoms::ok(), request_id).encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn close_enqueue(env: Env, sock_ref: ResourceArc<SocketRef>) -> Term {
    let _ = env;
    match runtime::close_enqueue(sock_ref) {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn shutdown(env: Env, sock_ref: ResourceArc<SocketRef>, how: Atom) -> Term {
    let how_int = if how == atoms::read() {
        libc::SHUT_RD
    } else if how == atoms::write() {
        libc::SHUT_WR
    } else if how == atoms::read_write() {
        libc::SHUT_RDWR
    } else {
        return (atoms::error(), atoms::einval()).encode(env);
    };

    match runtime::shutdown(sock_ref, how_int) {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn shutdown_async(env: Env, sock_ref: ResourceArc<SocketRef>, how: Atom) -> Term {
    shutdown_impl(env, sock_ref, how, false)
}

#[rustler::nif]
pub fn shutdown_enqueue(env: Env, sock_ref: ResourceArc<SocketRef>, how: Atom) -> Term {
    shutdown_impl(env, sock_ref, how, true)
}

fn shutdown_impl(env: Env, sock_ref: ResourceArc<SocketRef>, how: Atom, immediate: bool) -> Term {
    let how_int = if how == atoms::read() {
        libc::SHUT_RD
    } else if how == atoms::write() {
        libc::SHUT_WR
    } else if how == atoms::read_write() {
        libc::SHUT_RDWR
    } else {
        return (atoms::error(), atoms::einval()).encode(env);
    };

    if immediate {
        match runtime::shutdown_enqueue(sock_ref, how_int) {
            Ok(()) => atoms::ok().encode(env),
            Err(e) => err_atom(env, &e),
        }
    } else {
        match runtime::shutdown_async(sock_ref, how_int, env.pid()) {
            Ok(request_id) => (atoms::ok(), request_id).encode(env),
            Err(e) => err_atom(env, &e),
        }
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn setopts<'a>(
    env: Env<'a>,
    sock_ref: ResourceArc<SocketRef>,
    opts: Vec<(Atom, Term<'a>)>,
) -> Term<'a> {
    let SocketKind::Session(session) = &sock_ref.kind else {
        return (atoms::error(), atoms::einval()).encode(env);
    };

    for (key, val) in opts {
        if key == atoms::active() {
            let new_mode = if val == atoms::true_().encode(env) {
                ActiveMode::True
            } else if val == atoms::false_().encode(env) {
                ActiveMode::False
            } else if val == atoms::once().encode(env) {
                ActiveMode::Once
            } else if let Ok(b) = val.decode::<bool>() {
                if b {
                    ActiveMode::True
                } else {
                    ActiveMode::False
                }
            } else if let Ok(n) = val.decode::<i32>() {
                if n > 0 {
                    ActiveMode::N(n)
                } else {
                    return (atoms::error(), atoms::einval()).encode(env);
                }
            } else {
                return (atoms::error(), atoms::einval()).encode(env);
            };

            match runtime::set_active(sock_ref.clone(), new_mode.clone()) {
                Ok(()) => {
                    session
                        .opts
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .active = new_mode;
                }
                Err(e) => return err_atom(env, &e),
            }
        } else if key == atoms::nodelay() {
            let enabled = match val.decode::<bool>() {
                Ok(enabled) => enabled,
                Err(_) => return (atoms::error(), atoms::einval()).encode(env),
            };
            match runtime::set_nodelay(sock_ref.clone(), enabled) {
                Ok(()) => {
                    session
                        .opts
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .nodelay = enabled;
                }
                Err(e) => return err_atom(env, &e),
            }
        } else if key == atoms::buffer() {
            let n = match val.decode::<usize>() {
                Ok(n) => n,
                Err(_) => return (atoms::error(), atoms::einval()).encode(env),
            };
            session
                .opts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .buffer = n;
        } else if key == atoms::packet() {
            if val == atoms::raw().encode(env) || val.decode::<i32>().ok() == Some(0) {
                session
                    .opts
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .packet = PacketMode::Raw;
            } else {
                return (atoms::error(), atoms::einval()).encode(env);
            }
        }
    }

    atoms::ok().encode(env)
}

#[rustler::nif]
pub fn getopts<'a>(
    env: Env<'a>,
    sock_ref: ResourceArc<SocketRef>,
    opt_names: Vec<Atom>,
) -> Term<'a> {
    let SocketKind::Session(session) = &sock_ref.kind else {
        return (atoms::error(), atoms::einval()).encode(env);
    };
    let opts = session
        .opts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut result: Vec<(Atom, Term<'a>)> = Vec::with_capacity(opt_names.len());

    for name in opt_names {
        if name == atoms::active() {
            let val = match opts.active {
                ActiveMode::False => atoms::false_().encode(env),
                ActiveMode::True => atoms::true_().encode(env),
                ActiveMode::Once => atoms::once().encode(env),
                ActiveMode::N(n) => n.encode(env),
            };
            result.push((name, val));
        } else if name == atoms::nodelay() {
            result.push((name, opts.nodelay.encode(env)));
        } else if name == atoms::buffer() {
            result.push((name, opts.buffer.encode(env)));
        } else if name == atoms::packet() {
            result.push((name, atoms::raw().encode(env)));
        }
    }

    (atoms::ok(), result).encode(env)
}

#[rustler::nif]
pub fn peername(env: Env, sock_ref: ResourceArc<SocketRef>) -> Term {
    match &sock_ref.kind {
        SocketKind::Session(session) => match resolve_session_addr(session, false) {
            Ok(addr) => (atoms::ok(), encode_addr(env, &addr)).encode(env),
            Err(e) => err_atom(env, &e),
        },
        SocketKind::Listener(_) => (atoms::error(), atoms::einval()).encode(env),
    }
}

#[rustler::nif]
pub fn sockname(env: Env, sock_ref: ResourceArc<SocketRef>) -> Term {
    match &sock_ref.kind {
        SocketKind::Session(session) => match resolve_session_addr(session, true) {
            Ok(addr) => (atoms::ok(), encode_addr(env, &addr)).encode(env),
            Err(e) => err_atom(env, &e),
        },
        SocketKind::Listener(listener) => {
            (atoms::ok(), encode_addr(env, &listener.local)).encode(env)
        }
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn controlling_process(env: Env, sock_ref: ResourceArc<SocketRef>, pid: LocalPid) -> Term {
    let session = match &sock_ref.kind {
        SocketKind::Session(session) => session,
        SocketKind::Listener(_) => return (atoms::error(), atoms::einval()).encode(env),
    };
    let session_id = session.session_id;
    match runtime::update_owner(sock_ref.clone(), pid) {
        Ok(()) => {
            *session
                .owner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = pid;
            atoms::ok().encode(env)
        }
        Err(e) => {
            log::warn!(
                "failed to update owner for session {} in runtime",
                session_id
            );
            err_atom(env, &e)
        }
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
pub fn owner_down(env: Env, sock_ref: ResourceArc<SocketRef>) -> Term {
    match runtime::owner_down(sock_ref) {
        Ok(()) => atoms::ok().encode(env),
        Err(e) => err_atom(env, &e),
    }
}

#[rustler::nif]
pub fn debug_socket_id<'a>(env: Env<'a>, sock_ref: ResourceArc<SocketRef>) -> Term<'a> {
    match &sock_ref.kind {
        SocketKind::Listener(listener) => {
            (atoms::ok(), (atoms::listener(), listener.listener_id)).encode(env)
        }
        SocketKind::Session(session) => (
            atoms::ok(),
            (atoms::session(), session.shard, session.session_id),
        )
            .encode(env),
    }
}

fn encode_addr<'a>(env: Env<'a>, addr: &NetAddr) -> Term<'a> {
    match addr {
        NetAddr::V4(ip, port) => {
            let ip_tuple: Term<'a> = (ip[0], ip[1], ip[2], ip[3]).encode(env);
            (ip_tuple, *port).encode(env)
        }
        NetAddr::V6(segments, port) => {
            let ip = Ipv6Addr::from(*segments);
            let s = ip.segments();
            let raw_terms: Vec<rustler::sys::ERL_NIF_TERM> =
                s.iter().map(|seg| seg.encode(env).as_c_arg()).collect();
            // SAFETY: `raw_terms` comes from `encode(env)` in the same environment and remains
            // alive for this call; `make_tuple` only reads that slice while constructing the term.
            let ip_tuple = unsafe {
                let raw = rustler::wrapper::tuple::make_tuple(env.as_c_arg(), &raw_terms);
                rustler::Term::new(env, raw)
            };
            (ip_tuple, *port).encode(env)
        }
    }
}

fn resolve_session_addr(
    session: &crate::socket::SessionState,
    local: bool,
) -> Result<NetAddr, NifError> {
    let cache = if local { &session.local } else { &session.peer };
    if let Some(addr) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
    {
        return Ok(addr);
    }
    let addr = if local {
        local_addr_of_fd(session.fd)?
    } else {
        peer_addr_of_fd(session.fd)?
    };
    *cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(addr.clone());
    Ok(addr)
}

fn err_atom<'a>(env: Env<'a>, e: &NifError) -> Term<'a> {
    encode_error(env, e)
}
