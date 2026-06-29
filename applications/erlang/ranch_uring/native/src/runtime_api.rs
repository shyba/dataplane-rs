// DP-CS-0228: Explicit imports instead of wildcard
#[cfg(not(feature = "exec-strategy-sqpoll"))]
#[allow(unused_imports)]
use super::send_many_to_shard_linked;
#[allow(unused_imports)]
use super::{
    caller_link_id, current_thread_shard, flush_staged_commands_all, next_request_id_for_shard,
    recv_clock_raw, send_many_to_shard, send_session_result, send_to_shard, send_to_shard_linked,
    send_unit_result, session_link_id, shard_for_fd, shard_for_subscribe, shard_from_routed_id,
    ActiveMode, BatchOp, Command, NifError, ResourceArc, Result, ResultTarget, Runtime,
    RuntimeStatsSnapshot, SessionState, SubscribeControl, SubscribeOperation,
    NEXT_CONTROL_REQUEST_ID, RUNTIME,
};
use crate::socket::{SocketKind, SocketRef};
// Note: Some imports are unused in certain feature configs - they are needed
// for conditional compilation within functions (not just at top level).
#[allow(unused_imports)]
use crossbeam_channel::{Receiver, Select};
#[allow(unused_imports)]
use rustler::LocalPid;
#[allow(unused_imports)]
use std::collections::VecDeque;
#[allow(unused_imports)]
use std::os::fd::RawFd;
#[allow(unused_imports)]
use std::sync::atomic::Ordering;
#[allow(unused_imports)]
use std::sync::mpsc;
#[allow(unused_imports)]
use std::thread;
#[allow(unused_imports)]
use std::time::Duration;

struct SessionApiGuard<'a> {
    session_state: &'a SessionState,
}

impl Drop for SessionApiGuard<'_> {
    fn drop(&mut self) {
        self.session_state.end_api_call();
    }
}

fn begin_session_api_call(session_state: &SessionState) -> Result<SessionApiGuard<'_>> {
    if session_state.begin_api_call() {
        Ok(SessionApiGuard { session_state })
    } else {
        Err(NifError::Closed)
    }
}

fn send_session_command(session_state: &SessionState, command: Command) -> Result<()> {
    let _guard = begin_session_api_call(session_state)?;
    send_to_shard(session_state.shard, command)
}

fn send_session_command_linked(session_state: &SessionState, command: Command) -> Result<()> {
    let _guard = begin_session_api_call(session_state)?;
    send_to_shard_linked(
        session_state.shard,
        session_link_id(session_state.session_id),
        command,
    )
}

fn enqueue_session_command(session_state: &SessionState, command: Command) -> Result<()> {
    let _guard = begin_session_api_call(session_state)?;
    send_to_shard(session_state.shard, command)
}

fn enqueue_session_command_linked_unchecked(
    session_state: &SessionState,
    command: Command,
) -> Result<()> {
    send_to_shard_linked(
        session_state.shard,
        session_link_id(session_state.session_id),
        command,
    )
}

fn enqueue_session_command_unchecked(session_state: &SessionState, command: Command) -> Result<()> {
    send_to_shard(session_state.shard, command)
}

fn enqueue_session_command_linked(session_state: &SessionState, command: Command) -> Result<()> {
    let _guard = begin_session_api_call(session_state)?;
    send_to_shard_linked(
        session_state.shard,
        session_link_id(session_state.session_id),
        command,
    )
}

fn send_closed_session_command_linked(
    session_state: &SessionState,
    command: Command,
) -> Result<()> {
    send_to_shard_linked(
        session_state.shard,
        session_link_id(session_state.session_id),
        command,
    )
}

pub fn start_runtime() -> Result<()> {
    if let Some(runtime) = RUNTIME.get() {
        if runtime.is_stopping() {
            return Err(NifError::Closed);
        }
        return Ok(());
    }
    let runtime = Runtime::start()?;
    let _ = RUNTIME.set(runtime);
    Ok(())
}

pub fn stop_runtime() -> Result<()> {
    flush_staged_commands_all()?;
    let runtime = RUNTIME.get().ok_or(NifError::Closed)?;
    runtime.stop()
}

pub fn listen(owner: LocalPid, port: u16, backlog: i32) -> Result<ResourceArc<SocketRef>> {
    flush_staged_commands_all()?;
    let runtime = RUNTIME.get().ok_or(NifError::Closed)?;
    runtime.listen(owner, port, backlog)
}

pub fn accept(
    listener: ResourceArc<SocketRef>,
    owner: LocalPid,
    timeout_ms: i64,
) -> Result<ResourceArc<SocketRef>> {
    flush_staged_commands_all()?;
    let SocketKind::Listener(listener_state) = &listener.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    if listener_state.closed.load(Ordering::Acquire) {
        return Err(NifError::Closed);
    }

    let accepted = if timeout_ms < 0 {
        select_accept(&listener_state.accept_rxs, None)?
    } else {
        select_accept(
            &listener_state.accept_rxs,
            Some(Duration::from_millis(timeout_ms as u64)),
        )?
    };

    if let SocketKind::Session(session) = &accepted.kind {
        update_owner(accepted.clone(), owner)?;
        *session
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = owner;
    }

    Ok(accepted)
}

pub fn accept_async(
    listener: ResourceArc<SocketRef>,
    owner: LocalPid,
    timeout_ms: i64,
    reply_pid: LocalPid,
) -> Result<u64> {
    let request_id = NEXT_CONTROL_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    thread::spawn(move || {
        let result = accept(listener, owner, timeout_ms);
        send_session_result(&reply_pid, request_id, result);
    });
    Ok(request_id)
}

pub fn recv_async(session: ResourceArc<SocketRef>, len: usize, reply_pid: LocalPid) -> Result<u64> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let request_id = next_request_id_for_shard(session_state.shard)?;
    send_session_command_linked(
        session_state,
        Command::RecvAsync {
            session_id: session_state.session_id,
            len,
            request_id,
            enqueue_raw: recv_clock_raw(),
            target: ResultTarget::Erlang(reply_pid),
        },
    )?;
    Ok(request_id)
}

pub fn recv_sync(session: ResourceArc<SocketRef>, len: usize, timeout_ms: i64) -> Result<Vec<u8>> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let (tx, rx) = mpsc::sync_channel(1);
    send_session_command_linked(
        session_state,
        Command::RecvSync {
            session_id: session_state.session_id,
            len,
            enqueue_raw: recv_clock_raw(),
            reply: tx,
        },
    )?;

    if timeout_ms < 0 {
        rx.recv().map_err(|_| NifError::Closed)?
    } else {
        match rx.recv_timeout(Duration::from_millis(timeout_ms as u64)) {
            Ok(reply) => reply,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let _ = send_to_shard_linked(
                    session_state.shard,
                    session_link_id(session_state.session_id),
                    Command::CancelRecv {
                        session_id: session_state.session_id,
                        timed_out: true,
                    },
                );
                Err(NifError::Timeout)
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(NifError::Closed),
        }
    }
}

pub fn cancel_recv(session: ResourceArc<SocketRef>) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    send_session_command_linked(
        session_state,
        Command::CancelRecv {
            session_id: session_state.session_id,
            timed_out: false,
        },
    )
}

pub fn subscribe(reply_pid: LocalPid, operation: SubscribeOperation, fd: RawFd) -> Result<u64> {
    if fd < 0 {
        return Err(NifError::from_errno(libc::EINVAL));
    }
    flush_staged_commands_all()?;
    let runtime = RUNTIME.get().ok_or(NifError::Closed)?;
    let shard = shard_for_subscribe(operation, fd, runtime.senders.len());
    let subscription_id = next_request_id_for_shard(shard)?;
    runtime.senders[shard].send(Command::Subscribe {
        subscription_id,
        target: ResultTarget::Erlang(reply_pid),
        operation,
        fd,
    })?;
    Ok(subscription_id)
}

pub fn subscribe_control(subscription_id: u64, control: SubscribeControl) -> Result<()> {
    flush_staged_commands_all()?;
    let runtime = RUNTIME.get().ok_or(NifError::Closed)?;
    let shard = shard_from_routed_id(subscription_id, runtime.senders.len())?;
    runtime.senders[shard].send(Command::SubscribeControl {
        subscription_id,
        control,
    })
}

pub fn subscribe_add_consumer(reply_pid: LocalPid, subscription_id: u64) -> Result<()> {
    flush_staged_commands_all()?;
    let runtime = RUNTIME.get().ok_or(NifError::Closed)?;
    let shard = shard_from_routed_id(subscription_id, runtime.senders.len())?;
    runtime.senders[shard].send(Command::SubscribeAddConsumer {
        subscription_id,
        target: ResultTarget::Erlang(reply_pid),
    })
}

pub fn send_async(
    session: ResourceArc<SocketRef>,
    data: Vec<u8>,
    reply_pid: LocalPid,
) -> Result<u64> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let request_id = next_request_id_for_shard(session_state.shard)?;
    send_session_command_linked(
        session_state,
        Command::SendAsync {
            session_id: session_state.session_id,
            data,
            request_id,
            target: ResultTarget::Erlang(reply_pid),
            sync_enqueue_raw: None,
        },
    )?;
    Ok(request_id)
}

pub fn send_enqueue(session: ResourceArc<SocketRef>, data: Vec<u8>) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    enqueue_session_command(
        session_state,
        Command::SendEnqueue {
            session_id: session_state.session_id,
            data,
        },
    )
}

pub fn send_many_enqueue(session: ResourceArc<SocketRef>, packets: Vec<Vec<u8>>) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let _guard = begin_session_api_call(session_state)?;
    let mut cmds = Vec::with_capacity(packets.len());
    for data in packets {
        cmds.push(Command::SendEnqueue {
            session_id: session_state.session_id,
            data,
        });
    }
    send_many_to_shard(session_state.shard, cmds)
}

pub fn batch_async(
    session: ResourceArc<SocketRef>,
    request_id: u64,
    reply_pid: LocalPid,
    ops: Vec<BatchOp>,
) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    send_session_command_linked(
        session_state,
        Command::BatchAsync {
            session_id: session_state.session_id,
            request_id,
            target: ResultTarget::Erlang(reply_pid),
            ops,
        },
    )
}

pub fn set_active_async(
    session: ResourceArc<SocketRef>,
    mode: ActiveMode,
    reply_pid: LocalPid,
) -> Result<u64> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let _guard = begin_session_api_call(session_state)?;
    session_state
        .opts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .active = mode.clone();
    let request_id = next_request_id_for_shard(session_state.shard)?;
    enqueue_session_command_linked_unchecked(
        session_state,
        Command::SetActiveAsync {
            session_id: session_state.session_id,
            mode,
            request_id,
            target: ResultTarget::Erlang(reply_pid),
        },
    )?;
    Ok(request_id)
}

pub fn set_active_enqueue(session: ResourceArc<SocketRef>, mode: ActiveMode) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let _guard = begin_session_api_call(session_state)?;
    session_state
        .opts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .active = mode.clone();
    enqueue_session_command_linked_unchecked(
        session_state,
        Command::SetActiveEnqueue {
            session_id: session_state.session_id,
            mode,
        },
    )
}

pub fn send_sync(session: ResourceArc<SocketRef>, data: Vec<u8>) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let (tx, rx) = mpsc::sync_channel(1);
    send_session_command_linked(
        session_state,
        Command::SendAsync {
            session_id: session_state.session_id,
            data,
            request_id: 0,
            target: ResultTarget::SyncUnit(tx),
            sync_enqueue_raw: Some(recv_clock_raw()),
        },
    )?;
    rx.recv().map_err(|_| NifError::Closed)?
}

pub fn set_active(session: ResourceArc<SocketRef>, mode: ActiveMode) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let _guard = begin_session_api_call(session_state)?;
    session_state
        .opts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .active = mode.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    send_session_command_linked(
        session_state,
        Command::SetActive {
            session_id: session_state.session_id,
            mode,
            reply: tx,
        },
    )?;
    rx.recv().map_err(|_| NifError::Closed)?
}

pub fn set_nodelay(session: ResourceArc<SocketRef>, enabled: bool) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let (tx, rx) = mpsc::sync_channel(1);
    send_session_command(
        session_state,
        Command::SetNoDelay {
            session_id: session_state.session_id,
            enabled,
            reply: tx,
        },
    )?;
    rx.recv().map_err(|_| NifError::Closed)?
}

pub fn set_nodelay_async(
    session: ResourceArc<SocketRef>,
    enabled: bool,
    reply_pid: LocalPid,
) -> Result<u64> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let request_id = next_request_id_for_shard(session_state.shard)?;
    send_session_command(
        session_state,
        Command::SetNoDelayAsync {
            session_id: session_state.session_id,
            enabled,
            request_id,
            target: ResultTarget::Erlang(reply_pid),
        },
    )?;
    Ok(request_id)
}

pub fn set_nodelay_enqueue(session: ResourceArc<SocketRef>, enabled: bool) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    enqueue_session_command(
        session_state,
        Command::SetNoDelayEnqueue {
            session_id: session_state.session_id,
            enabled,
        },
    )
}

pub fn set_mailbox_passive_enqueue(session: ResourceArc<SocketRef>, enabled: bool) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let _guard = begin_session_api_call(session_state)?;
    session_state
        .mailbox_passive
        .store(enabled, Ordering::Release);
    enqueue_session_command_unchecked(
        session_state,
        Command::SetMailboxPassiveEnqueue {
            session_id: session_state.session_id,
            enabled,
        },
    )
}

pub fn update_owner(session: ResourceArc<SocketRef>, pid: LocalPid) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let (tx, rx) = mpsc::sync_channel(1);
    send_session_command_linked(
        session_state,
        Command::UpdateOwner {
            session_id: session_state.session_id,
            pid,
            reply: tx,
        },
    )?;
    rx.recv().map_err(|_| NifError::Closed)?
}

pub(crate) fn owner_down(session: ResourceArc<SocketRef>) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    if !session_state.mark_api_closed() {
        return Ok(());
    }
    send_closed_session_command_linked(
        session_state,
        Command::OwnerDown {
            session_id: session_state.session_id,
        },
    )
}

pub fn shutdown(session: ResourceArc<SocketRef>, how: i32) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let (tx, rx) = mpsc::sync_channel(1);
    send_session_command_linked(
        session_state,
        Command::Shutdown {
            session_id: session_state.session_id,
            how,
            reply: tx,
        },
    )?;
    rx.recv().map_err(|_| NifError::Closed)?
}

pub fn shutdown_async(
    session: ResourceArc<SocketRef>,
    how: i32,
    reply_pid: LocalPid,
) -> Result<u64> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    let request_id = next_request_id_for_shard(session_state.shard)?;
    send_session_command_linked(
        session_state,
        Command::ShutdownAsync {
            session_id: session_state.session_id,
            how,
            request_id,
            target: ResultTarget::Erlang(reply_pid),
        },
    )?;
    Ok(request_id)
}

pub fn shutdown_enqueue(session: ResourceArc<SocketRef>, how: i32) -> Result<()> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    enqueue_session_command_linked(
        session_state,
        Command::ShutdownEnqueue {
            session_id: session_state.session_id,
            how,
        },
    )
}

pub fn debug_runtime_stats() -> Result<Vec<RuntimeStatsSnapshot>> {
    flush_staged_commands_all()?;
    let runtime = RUNTIME.get().ok_or(NifError::Closed)?;
    runtime.debug_runtime_stats()
}

pub fn close(handle: ResourceArc<SocketRef>) -> Result<()> {
    match &handle.kind {
        SocketKind::Listener(listener_state) => {
            listener_state.closed.store(true, Ordering::Release);
            let runtime = RUNTIME.get().ok_or(NifError::Closed)?;
            runtime.close_listener(listener_state.listener_id)
        }
        SocketKind::Session(session_state) => {
            if !session_state.mark_api_closed() {
                return Ok(());
            }
            let (tx, rx) = mpsc::sync_channel(1);
            let _ = send_to_shard_linked(
                session_state.shard,
                session_link_id(session_state.session_id),
                Command::Close {
                    session_id: session_state.session_id,
                    reason: NifError::Closed,
                    reply: Some(tx),
                },
            );
            let _ = rx.recv_timeout(Duration::from_secs(1));
            Ok(())
        }
    }
}

pub fn close_async(handle: ResourceArc<SocketRef>, reply_pid: LocalPid) -> Result<u64> {
    let request_id = NEXT_CONTROL_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    match &handle.kind {
        SocketKind::Listener(_listener_state) => {
            let listener = handle.clone();
            thread::spawn(move || {
                let result = close(listener);
                send_unit_result(&reply_pid, request_id, result);
            });
            Ok(request_id)
        }
        SocketKind::Session(session_state) => {
            session_state.mark_api_closed();
            send_to_shard_linked(
                session_state.shard,
                session_link_id(session_state.session_id),
                Command::CloseAsync {
                    session_id: session_state.session_id,
                    reason: NifError::Closed,
                    request_id,
                    target: ResultTarget::Erlang(reply_pid),
                },
            )?;
            Ok(request_id)
        }
    }
}

pub fn close_enqueue(handle: ResourceArc<SocketRef>) -> Result<()> {
    match &handle.kind {
        SocketKind::Listener(listener_state) => {
            listener_state.closed.store(true, Ordering::Release);
            if let Some(runtime) = RUNTIME.get() {
                runtime.close_listener(listener_state.listener_id)
            } else {
                Err(NifError::Closed)
            }
        }
        SocketKind::Session(session_state) => {
            if !session_state.mark_api_closed() {
                return Ok(());
            }
            send_to_shard_linked(
                session_state.shard,
                session_link_id(session_state.session_id),
                Command::CloseEnqueue {
                    session_id: session_state.session_id,
                    reason: NifError::Closed,
                },
            )
        }
    }
}

fn select_accept(
    accept_rxs: &[Receiver<ResourceArc<SocketRef>>],
    timeout: Option<Duration>,
) -> Result<ResourceArc<SocketRef>> {
    if accept_rxs.is_empty() {
        return Err(NifError::Closed);
    }
    let preferred = current_thread_shard(accept_rxs.len());
    if let Ok(sock) = accept_rxs[preferred].try_recv() {
        return Ok(sock);
    }

    let mut sel = Select::new();
    for rx in accept_rxs {
        sel.recv(rx);
    }

    let oper = match timeout {
        Some(timeout) => sel.select_timeout(timeout).map_err(|_| NifError::Timeout)?,
        None => sel.select(),
    };
    let index = oper.index();
    oper.recv(&accept_rxs[index]).map_err(|_| NifError::Closed)
}

pub fn alloc_request_id(session: &ResourceArc<SocketRef>) -> Result<u64> {
    let SocketKind::Session(session_state) = &session.kind else {
        return Err(NifError::from_errno(libc::EINVAL));
    };
    next_request_id_for_shard(session_state.shard)
}

pub fn bench_runtime_nop_async(reply_pid: LocalPid) -> Result<u64> {
    flush_staged_commands_all()?;
    let runtime = RUNTIME.get().ok_or(NifError::Closed)?;
    let shard = current_thread_shard(runtime.senders.len());
    let request_id = next_request_id_for_shard(shard)?;
    runtime.senders[shard].send(Command::linked(
        caller_link_id(&reply_pid),
        Command::NoopAsync {
            request_id,
            target: ResultTarget::Erlang(reply_pid),
        },
    ))?;
    Ok(request_id)
}

pub fn caller_shard() -> Result<usize> {
    let runtime = RUNTIME.get().ok_or(NifError::Closed)?;
    Ok(current_thread_shard(runtime.senders.len()))
}

pub fn stat_async_to(fd: RawFd, reply_pid: LocalPid) -> Result<u64> {
    if fd < 0 {
        return Err(NifError::from_errno(libc::EINVAL));
    }
    flush_staged_commands_all()?;
    let runtime = RUNTIME.get().ok_or(NifError::Closed)?;
    let shard = current_thread_shard(runtime.senders.len());
    let request_id = next_request_id_for_shard(shard)?;
    runtime.senders[shard].send(Command::linked(
        caller_link_id(&reply_pid),
        Command::StatAsync {
            fd,
            request_id,
            target: ResultTarget::Erlang(reply_pid),
        },
    ))?;
    Ok(request_id)
}
