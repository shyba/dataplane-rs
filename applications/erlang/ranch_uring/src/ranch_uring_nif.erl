-module(ranch_uring_nif).
-on_load(load_nif/0).

-export([init/0, stop_runtime/0]).
-export([ready/0]).
-export([debug_runtime_stats/0]).
-export([bench_direct_ok/0, bench_direct_binary/1, bench_zero_read/1, bench_env_send_ok/1, bench_env_send_binary/2, bench_zero_env_send_binary/2]).
-export([bench_owned_env_send_ok/1, bench_owned_env_send_binary/2, bench_zero_owned_env_send_binary/2]).
-export([bench_zero_owned_env_send_many/3, bench_zero_owned_env_send_batch/3]).
-export([bench_batch/1]).
-export([bench_queue_batch_async/1]).
-export([bench_runtime_nop_async/0]).
-export([caller_shard/0, stat_async/1, stat_async_to/2]).
-export([batch_async/2]).
-export([command/1]).
-export([listen/2, accept/2, accept_async/2, accept_async_to/3, recv/3, recv_async/2, recv_async_to/3, cancel_recv/1]).
-export([send/2, send_enqueue/2, send_enqueues/2, send_async/2, shutdown_async/2, close_async/1]).
-export([set_active_async/2, set_active_enqueue/2, set_nodelay_async/2, set_nodelay_enqueue/2, set_mailbox_passive_enqueue/2, sendfile/4]).
-export([close/1, close_enqueue/1, shutdown/2, shutdown_enqueue/2, setopts/2, getopts/2]).
-export([peername/1, sockname/1, controlling_process/2]).
-export([owner_down/1]).
-export([debug_socket_id/1]).

load_nif() ->
    case code:priv_dir(ranch_uring) of
        {error, _} -> {error, no_priv_dir};
        PrivDir ->
            NifPath = filename:join(PrivDir, "ranch_uring_nif"),
            erlang:load_nif(NifPath, 0)
    end.

init() ->
    erlang:nif_error(not_loaded).

stop_runtime() ->
    erlang:nif_error(not_loaded).

ready() ->
    try caller_shard() of
        {ok, _} ->
            ok;
        {error, _} = Error ->
            Error;
        Other ->
            {error, {unexpected_nif_ready_result, Other}}
    catch
        error:Reason ->
            {error, Reason}
    end.

debug_runtime_stats() ->
    erlang:nif_error(not_loaded).

bench_direct_ok() ->
    erlang:nif_error(not_loaded).

bench_direct_binary(_Size) ->
    erlang:nif_error(not_loaded).

bench_zero_read(_Size) ->
    erlang:nif_error(not_loaded).

bench_env_send_ok(_Pid) ->
    erlang:nif_error(not_loaded).

bench_env_send_binary(_Pid, _Size) ->
    erlang:nif_error(not_loaded).

bench_zero_env_send_binary(_Pid, _Size) ->
    erlang:nif_error(not_loaded).

bench_owned_env_send_ok(_Pid) ->
    erlang:nif_error(not_loaded).

bench_owned_env_send_binary(_Pid, _Size) ->
    erlang:nif_error(not_loaded).

bench_zero_owned_env_send_binary(_Pid, _Size) ->
    erlang:nif_error(not_loaded).

bench_zero_owned_env_send_many(_Pid, _Size, _Count) ->
    erlang:nif_error(not_loaded).

bench_zero_owned_env_send_batch(_Pid, _Size, _Count) ->
    erlang:nif_error(not_loaded).

bench_batch(_Commands) ->
    erlang:nif_error(not_loaded).

bench_queue_batch_async(_Commands) ->
    erlang:nif_error(not_loaded).

bench_runtime_nop_async() ->
    erlang:nif_error(not_loaded).

caller_shard() ->
    erlang:nif_error(not_loaded).

stat_async(_Fd) ->
    erlang:nif_error(not_loaded).

stat_async_to(_Fd, _ReplyPid) ->
    erlang:nif_error(not_loaded).

batch_async(_SockRef, _Commands) ->
    erlang:nif_error(not_loaded).

command(_Command) ->
    erlang:nif_error(not_loaded).

listen(_Port, _Backlog) ->
    erlang:nif_error(not_loaded).

accept(_ListenRef, _Timeout) ->
    erlang:nif_error(not_loaded).

accept_async(_ListenRef, _Timeout) ->
    erlang:nif_error(not_loaded).

accept_async_to(_ListenRef, _Timeout, _ReplyPid) ->
    erlang:nif_error(not_loaded).

recv(_SockRef, _Length, _Timeout) ->
    erlang:nif_error(not_loaded).

recv_async(_SockRef, _Length) ->
    erlang:nif_error(not_loaded).

recv_async_to(_SockRef, _Length, _ReplyPid) ->
    erlang:nif_error(not_loaded).

cancel_recv(_SockRef) ->
    erlang:nif_error(not_loaded).

send(_SockRef, _Data) ->
    erlang:nif_error(not_loaded).

send_enqueue(_SockRef, _Data) ->
    erlang:nif_error(not_loaded).

send_enqueues(_SockRef, _Packets) ->
    erlang:nif_error(not_loaded).

send_async(_SockRef, _Data) ->
    erlang:nif_error(not_loaded).

set_active_async(_SockRef, _Mode) ->
    erlang:nif_error(not_loaded).

set_active_enqueue(_SockRef, _Mode) ->
    erlang:nif_error(not_loaded).

set_nodelay_async(_SockRef, _Enabled) ->
    erlang:nif_error(not_loaded).

set_nodelay_enqueue(_SockRef, _Enabled) ->
    erlang:nif_error(not_loaded).

set_mailbox_passive_enqueue(_SockRef, _Enabled) ->
    erlang:nif_error(not_loaded).

sendfile(_SockRef, _Path, _Offset, _Length) ->
    erlang:nif_error(not_loaded).

close(_Ref) ->
    erlang:nif_error(not_loaded).

close_enqueue(_Ref) ->
    erlang:nif_error(not_loaded).

close_async(_Ref) ->
    erlang:nif_error(not_loaded).

shutdown(_SockRef, _How) ->
    erlang:nif_error(not_loaded).

shutdown_enqueue(_SockRef, _How) ->
    erlang:nif_error(not_loaded).

shutdown_async(_SockRef, _How) ->
    erlang:nif_error(not_loaded).

setopts(_Ref, _Opts) ->
    erlang:nif_error(not_loaded).

getopts(_Ref, _OptNames) ->
    erlang:nif_error(not_loaded).

peername(_SockRef) ->
    erlang:nif_error(not_loaded).

sockname(_Ref) ->
    erlang:nif_error(not_loaded).

controlling_process(_Ref, _Pid) ->
    erlang:nif_error(not_loaded).

owner_down(_Ref) ->
    erlang:nif_error(not_loaded).

debug_socket_id(_Ref) ->
    erlang:nif_error(not_loaded).
