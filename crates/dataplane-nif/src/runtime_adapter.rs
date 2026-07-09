//! Runtime adapter — Rustler result encoding and delivery ownership.
//!
//! This module was extracted from the ranch_uring native crate as part of
//! the dataplane-nif extraction boundary (DP-NIF-0031).

use dataplane_runtime::errors::{NifError, Result};
use dataplane_runtime::runtime_protocol::BatchResult;
use dataplane_runtime::socket::SocketRef;
use rustler::{Encoder, LocalPid, OwnedBinary, OwnedEnv, ResourceArc};
use std::sync::mpsc::SyncSender;

mod atoms {
    rustler::atoms! {
        ok,
        error,
        reply,
        nif_results,
        batch_reply,
        uring_tcp,
        uring_tcp_closed,
        uring_tcp_passive,
        uring_tcp_passive_data,
        uring_tcp_passive_closed,
        uring_tcp_passive_error,
    }
}

pub enum AsyncReplyPayload {
    Ok,
    U64(u64),
    Data(Vec<u8>),
    DataOwned(OwnedBinary),
    Session(ResourceArc<SocketRef>),
    Error(NifError),
}

#[derive(Clone)]
pub enum ResultTarget {
    Erlang(LocalPid),
    SyncUnit(SyncSender<Result<()>>),
}

impl PartialEq for ResultTarget {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Erlang(a), Self::Erlang(b)) => a == b,
            (Self::SyncUnit(a), Self::SyncUnit(b)) => std::ptr::eq(a, b),
            _ => false,
        }
    }
}

impl Eq for ResultTarget {}

pub fn result_target_is_local_sync(target: &ResultTarget) -> bool {
    matches!(target, ResultTarget::SyncUnit(_))
}

fn owned_binary_from_slice(data: &[u8]) -> Option<OwnedBinary> {
    let mut bin = OwnedBinary::new(data.len())?;
    bin.as_mut_slice().copy_from_slice(data);
    Some(bin)
}

fn enomem_atom(env: rustler::Env<'_>) -> rustler::types::atom::Atom {
    let atom_name = NifError::from_errno(libc::ENOMEM).atom_name();
    rustler::types::atom::Atom::from_str(env, atom_name).unwrap_or_else(|_| atoms::error())
}

fn send_and_clear<'a, F, T>(env: &mut OwnedEnv, pid: &LocalPid, closure: F) -> bool
where
    F: FnOnce(rustler::Env<'a>) -> T,
    T: Encoder,
{
    if rustler::thread::is_scheduler_thread() {
        return false;
    }

    env.send_and_clear(pid, closure).is_ok()
}

struct ErlangResultSink;

impl ErlangResultSink {
    fn flush_pid(pid: &LocalPid, results: Vec<(u64, AsyncReplyPayload)>) -> bool {
        let mut env = OwnedEnv::new();
        send_and_clear(&mut env, pid, |env| {
            let encoded: Vec<_> = results
                .into_iter()
                .map(|(request_id, payload)| {
                    (request_id, encode_async_reply(env, payload)).encode(env)
                })
                .collect();
            (atoms::nif_results(), encoded).encode(env)
        })
    }
}

pub fn send_result_batch(pid: &LocalPid, results: Vec<(u64, AsyncReplyPayload)>) -> bool {
    ErlangResultSink::flush_pid(pid, results)
}

pub fn send_result_single(pid: &LocalPid, request_id: u64, payload: AsyncReplyPayload) -> bool {
    let mut env = OwnedEnv::new();
    send_and_clear(&mut env, pid, |env| {
        (atoms::reply(), request_id, encode_async_reply(env, payload)).encode(env)
    })
}

pub fn send_result_single_to(
    target: &ResultTarget,
    request_id: u64,
    payload: AsyncReplyPayload,
) -> bool {
    match target {
        ResultTarget::Erlang(pid) => send_result_single(pid, request_id, payload),
        ResultTarget::SyncUnit(reply) => reply.send(unit_result_from_payload(payload)).is_ok(),
    }
}

pub fn send_result_batch_to(target: &ResultTarget, results: Vec<(u64, AsyncReplyPayload)>) -> bool {
    match target {
        ResultTarget::Erlang(pid) => send_result_batch(pid, results),
        ResultTarget::SyncUnit(_) => results.is_empty(),
    }
}

pub fn target_owner_pid(target: &ResultTarget) -> LocalPid {
    match target {
        ResultTarget::Erlang(pid) => *pid,
        ResultTarget::SyncUnit(_) => unreachable!("local sync result target has no Erlang owner"),
    }
}

fn unit_result_from_payload(payload: AsyncReplyPayload) -> Result<()> {
    match payload {
        AsyncReplyPayload::Ok => Ok(()),
        AsyncReplyPayload::Error(err) => Err(err),
        _ => Err(NifError::from_errno(libc::EINVAL)),
    }
}

pub fn send_unit_result(pid: &LocalPid, request_id: u64, result: Result<()>) {
    let payload = match result {
        Ok(()) => AsyncReplyPayload::Ok,
        Err(err) => AsyncReplyPayload::Error(err),
    };
    let _ = send_result_single(pid, request_id, payload);
}

pub fn send_session_result(
    pid: &LocalPid,
    request_id: u64,
    result: Result<ResourceArc<SocketRef>>,
) {
    let payload = match result {
        Ok(session) => AsyncReplyPayload::Session(session),
        Err(err) => AsyncReplyPayload::Error(err),
    };
    let _ = send_result_single(pid, request_id, payload);
}

pub fn send_batch_reply(pid: &LocalPid, request_id: u64, results: Vec<(u64, BatchResult)>) {
    let mut env = OwnedEnv::new();
    let _ = send_and_clear(&mut env, pid, |env| {
        let encoded: Vec<_> = results
            .into_iter()
            .map(|(id, result)| match result {
                BatchResult::Ok => (id, atoms::ok()).encode(env),
                BatchResult::Data(data) => match owned_binary_from_slice(&data) {
                    Some(bin) => {
                        (id, (atoms::ok(), rustler::Binary::from_owned(bin, env))).encode(env)
                    }
                    None => (id, (atoms::error(), enomem_atom(env))).encode(env),
                },
                BatchResult::Error(err) => {
                    let err = NifError::from(err);
                    let atom = rustler::types::atom::Atom::from_str(env, err.atom_name())
                        .unwrap_or_else(|_| atoms::error());
                    (id, (atoms::error(), atom)).encode(env)
                }
            })
            .collect();
        (atoms::batch_reply(), request_id, encoded).encode(env)
    });
}

pub fn send_batch_reply_to(
    target: &ResultTarget,
    request_id: u64,
    results: Vec<(u64, BatchResult)>,
) {
    match target {
        ResultTarget::Erlang(pid) => send_batch_reply(pid, request_id, results),
        ResultTarget::SyncUnit(_) => {}
    }
}

pub fn send_active_data_message(handle: &ResourceArc<SocketRef>, pid: &LocalPid, data: &[u8]) {
    let mut env = OwnedEnv::new();
    let Some(bin) = owned_binary_from_slice(data) else {
        return;
    };
    let _ = send_and_clear(&mut env, pid, |env| {
        (
            atoms::uring_tcp(),
            handle.clone(),
            rustler::Binary::from_owned(bin, env),
        )
            .encode(env)
    });
}

pub fn send_active_passive_message(handle: &ResourceArc<SocketRef>, pid: &LocalPid) {
    let mut env = OwnedEnv::new();
    let _ = send_and_clear(&mut env, pid, |env| {
        (atoms::uring_tcp_passive(), handle.clone()).encode(env)
    });
}

pub fn send_passive_data_message(handle: &ResourceArc<SocketRef>, pid: &LocalPid, data: &[u8]) {
    let mut env = OwnedEnv::new();
    let Some(bin) = owned_binary_from_slice(data) else {
        return;
    };
    let _ = send_and_clear(&mut env, pid, |env| {
        (
            atoms::uring_tcp_passive_data(),
            handle.clone(),
            rustler::Binary::from_owned(bin, env),
        )
            .encode(env)
    });
}

pub fn send_passive_closed_message(handle: &ResourceArc<SocketRef>, pid: &LocalPid) {
    let mut env = OwnedEnv::new();
    let _ = send_and_clear(&mut env, pid, |env| {
        (atoms::uring_tcp_passive_closed(), handle.clone()).encode(env)
    });
}

pub fn send_passive_error_message(handle: &ResourceArc<SocketRef>, pid: &LocalPid, err: &NifError) {
    let mut env = OwnedEnv::new();
    let _ = send_and_clear(&mut env, pid, |env| {
        let atom = rustler::types::atom::Atom::from_str(env, err.atom_name())
            .unwrap_or_else(|_| atoms::error());
        (atoms::uring_tcp_passive_error(), handle.clone(), atom).encode(env)
    });
}

pub fn send_active_closed_message(handle: &ResourceArc<SocketRef>, pid: &LocalPid) {
    let mut env = OwnedEnv::new();
    let _ = send_and_clear(&mut env, pid, |env| {
        (atoms::uring_tcp_closed(), handle.clone()).encode(env)
    });
}

fn encode_async_reply<'a>(env: rustler::Env<'a>, payload: AsyncReplyPayload) -> rustler::Term<'a> {
    match payload {
        AsyncReplyPayload::Ok => atoms::ok().encode(env),
        AsyncReplyPayload::U64(value) => (atoms::ok(), value).encode(env),
        AsyncReplyPayload::Data(data) => match owned_binary_from_slice(&data) {
            Some(bin) => (atoms::ok(), rustler::Binary::from_owned(bin, env)).encode(env),
            None => {
                let err = enomem_atom(env);
                (atoms::error(), err).encode(env)
            }
        },
        AsyncReplyPayload::DataOwned(bin) => {
            (atoms::ok(), rustler::Binary::from_owned(bin, env)).encode(env)
        }
        AsyncReplyPayload::Session(session) => (atoms::ok(), session).encode(env),
        AsyncReplyPayload::Error(err) => {
            let atom = rustler::types::atom::Atom::from_str(env, err.atom_name())
                .unwrap_or_else(|_| atoms::error());
            (atoms::error(), atom).encode(env)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn sync_unit_target_delivers_ok_payload() {
        let (tx, rx) = mpsc::sync_channel(1);
        let target = ResultTarget::SyncUnit(tx);

        assert!(send_result_single_to(&target, 99, AsyncReplyPayload::Ok));

        assert!(rx.try_recv().expect("sync result should arrive").is_ok());
    }

    #[test]
    fn sync_unit_target_delivers_error_payload() {
        let (tx, rx) = mpsc::sync_channel(1);
        let target = ResultTarget::SyncUnit(tx);

        assert!(send_result_single_to(
            &target,
            99,
            AsyncReplyPayload::Error(NifError::Closed)
        ));

        let err = rx
            .try_recv()
            .expect("sync result should arrive")
            .expect_err("sync result should be an error");
        assert_eq!(err.atom_name(), NifError::Closed.atom_name());
    }

    #[test]
    fn sync_unit_target_rejects_non_unit_payload_as_einval() {
        let (tx, rx) = mpsc::sync_channel(1);
        let target = ResultTarget::SyncUnit(tx);

        assert!(send_result_single_to(
            &target,
            99,
            AsyncReplyPayload::Data(Vec::new())
        ));

        let err = rx
            .try_recv()
            .expect("sync result should arrive")
            .expect_err("sync result should be an error");
        assert_eq!(
            err.atom_name(),
            NifError::from_errno(libc::EINVAL).atom_name()
        );
    }
}
