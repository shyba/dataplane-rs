//! Shard send-path helpers.
//!
//! Sends commands to runtime shards through the shared command channel.

use super::{current_runtime, Command, NifError, Result};

/// Flush any commands that have been staged for batched delivery.
///
/// There is no staged queue in the current send path, so this is intentionally
/// a no-op compatibility hook for call sites that flush before direct sends.
pub(crate) fn flush_staged_commands_all() -> Result<()> {
    Ok(())
}

/// Send a single `Command` to `shard`.
///
/// Flushes staged commands first, then checks the runtime is still running
/// before delegating to the shard's `Sender`.
pub(crate) fn send_to_shard(shard: usize, cmd: Command) -> Result<()> {
    flush_staged_commands_all()?;
    let runtime = current_runtime()?;
    if runtime.is_stopping() {
        return Err(NifError::Closed);
    }
    runtime.senders[shard].send(cmd)
}

/// Send a single `Command` to `shard` with an attached link ID.
///
/// This is equivalent to `send_to_shard(shard, Command::linked(link, cmd))`
/// but is provided as a convenience to keep call-sites readable.
pub(crate) fn send_to_shard_linked(shard: usize, link: u64, cmd: Command) -> Result<()> {
    send_to_shard(shard, Command::linked(link, cmd))
}

/// Send multiple `Command`s to `shard` in a single batch.
///
/// Flushes staged commands first, then delivers the full vector via
/// `ShardSender::send_many`.
pub(crate) fn send_many_to_shard(shard: usize, cmds: Vec<Command>) -> Result<()> {
    flush_staged_commands_all()?;
    let runtime = current_runtime()?;
    if runtime.is_stopping() {
        return Err(NifError::Closed);
    }
    runtime.senders[shard].send_many(cmds)
}

/// Send multiple `Command`s to `shard`, each with an attached link ID.
#[allow(dead_code)]
pub(crate) fn send_many_to_shard_linked(shard: usize, link: u64, cmds: Vec<Command>) -> Result<()> {
    let linked_cmds = cmds
        .into_iter()
        .map(|cmd| Command::linked(link, cmd))
        .collect();
    send_many_to_shard(shard, linked_cmds)
}
