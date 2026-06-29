-module(ranch_tcp_uring).
-behaviour(ranch_transport).

-export([name/0]).
-export([secure/0]).
-export([messages/0]).
-export([listen/1]).
-export([disallowed_listen_options/0]).
-export([accept/2]).
-export([handshake/2]).
-export([handshake/3]).
-export([handshake_continue/2]).
-export([handshake_continue/3]).
-export([handshake_cancel/1]).
-export([connect/3]).
-export([connect/4]).
-export([recv/3]).
-export([recv_proxy_header/2]).
-export([send/2]).
-export([batch/2, batch/3]).
-export([sendv/2]).
-export([sendfile/2]).
-export([sendfile/4]).
-export([sendfile/5]).
-export([setopts/2]).
-export([getopts/2]).
-export([getstat/1]).
-export([getstat/2]).
-export([controlling_process/2]).
-export([peername/1]).
-export([sockname/1]).
-export([shutdown/2]).
-export([close/1]).
-export([cleanup/1]).

-ifdef(RANCH_TCP_URING_TRACE).
-define(TRACE_EVENT(Name, FieldsExpr), trace_event(Name, FieldsExpr)).
-define(TRACE_RESULT(Name, ResultExpr, FieldsExpr), trace_result(Name, ResultExpr, FieldsExpr)).
-define(TRACE_RUNTIME_RESULT(RequestId, ResultExpr, SocketExpr), trace_runtime_result(RequestId, ResultExpr, SocketExpr)).
-else.
-define(TRACE_EVENT(_Name, _FieldsExpr), ok).
-define(TRACE_RESULT(_Name, _ResultExpr, _FieldsExpr), ok).
-define(TRACE_RUNTIME_RESULT(_RequestId, _ResultExpr, _SocketExpr), ok).
-compile({nowarn_unused_function, [
    {first_pair_id, 1},
    {trace_runtime_result, 3},
    {trace_reply_ok, 2},
    {trace_result, 3},
    {trace_event, 2},
    {ref_trace_fields, 1},
    {maybe_ref_trace_fields, 2}
]}).
-endif.

-define(OWNER_WATCH_TABLE, ranch_uring_owner_watch).

-type opt() :: {backlog, non_neg_integer()}
    | {buffer, non_neg_integer()}
    | {delay_send, boolean()}
    | {dontroute, boolean()}
    | {exit_on_close, boolean()}
    | {fd, non_neg_integer()}
    | {high_msgq_watermark, non_neg_integer()}
    | {high_watermark, non_neg_integer()}
    | inet
    | inet6
    | {ip, inet:ip_address() | inet:local_address()}
    | {ipv6_v6only, boolean()}
    | {keepalive, boolean()}
    | {linger, {boolean(), non_neg_integer()}}
    | {low_msgq_watermark, non_neg_integer()}
    | {low_watermark, non_neg_integer()}
    | {nodelay, boolean()}
    | {port, inet:port_number()}
    | {priority, integer()}
    | {raw, non_neg_integer(), non_neg_integer(), binary()}
    | {recbuf, non_neg_integer()}
    | {send_timeout, timeout()}
    | {send_timeout_close, boolean()}
    | {sndbuf, non_neg_integer()}
    | {tos, integer()}.
-export_type([opt/0]).

-type opts() :: [opt()].
-export_type([opts/0]).

-spec name() -> tcp_uring.
name() -> tcp_uring.

-spec secure() -> boolean().
secure() -> false.

-spec messages() -> {uring_tcp, uring_tcp_closed, uring_tcp_error, uring_tcp_passive}.
messages() -> {uring_tcp, uring_tcp_closed, uring_tcp_error, uring_tcp_passive}.

-spec listen(ranch:transport_opts(opts())) -> {ok, any()} | {error, atom()}.
listen(TransOpts) ->
    ok = cleanup(TransOpts),
    Logger = maps:get(logger, TransOpts, logger),
    SocketOpts0 = maps:get(socket_opts, TransOpts, []),
    SocketOpts = prepare_socket_opts(SocketOpts0, Logger),
    Port = proplists:get_value(port, SocketOpts, 0),
    Backlog = proplists:get_value(backlog, SocketOpts, 1024),
    Result = ranch_uring_nif:listen(Port, Backlog),
    ?TRACE_RESULT(listen_reply, Result, [{port, Port}, {backlog, Backlog}]),
    Result.

prepare_socket_opts([Backend = {inet_backend, _} | SocketOpts], Logger) ->
    [Backend | prepare_socket_opts(SocketOpts, Logger)];
prepare_socket_opts(SocketOpts0, Logger) ->
    SocketOpts1 = ranch:set_option_default(SocketOpts0, backlog, 1024),
    SocketOpts2 = ranch:set_option_default(SocketOpts1, nodelay, true),
    SocketOpts3 = ranch:set_option_default(SocketOpts2, send_timeout, 30000),
    SocketOpts4 = ranch:set_option_default(SocketOpts3, send_timeout_close, true),
    ranch:filter_options(SocketOpts4, disallowed_listen_options(),
        [binary, {active, false}, {packet, raw}, {reuseaddr, true}], Logger).

-spec disallowed_listen_options() -> [atom()].
disallowed_listen_options() ->
    [active, header, mode, packet, packet_size, line_delimiter, reuseaddr].

-spec accept(any(), timeout()) -> {ok, any()} | {error, closed | timeout | atom()}.
accept(ListenRef, infinity) ->
    accept_loop(ListenRef);
accept(ListenRef, Timeout) ->
    TimeoutMs = timeout_to_ms(Timeout),
    case submit_accept_request(ListenRef, TimeoutMs) of
        {ok, {demux, Future}} ->
            RequestId = future_request_id(Future),
            _ = RequestId,
            ?TRACE_EVENT(accept_req, [{request_id, RequestId}, {timeout_ms, TimeoutMs} | ref_trace_fields(ListenRef)]),
            await_accept_reply(Future, ListenRef, TimeoutMs);
        {ok, {local, RequestId}} ->
            ?TRACE_EVENT(accept_req, [{request_id, RequestId}, {timeout_ms, TimeoutMs} | ref_trace_fields(ListenRef)]),
            case await_runtime_reply(RequestId, self(), TimeoutMs, accept) of
                {ok, Socket} = Result ->
                    register_owner_watch(Socket, self()),
                    Result;
                Result ->
                    Result
            end;
        {error, _} = Err ->
            ?TRACE_RESULT(accept_req_error, Err, [{timeout_ms, TimeoutMs} | ref_trace_fields(ListenRef)]),
            Err
    end.

accept_loop(ListenRef) ->
    case submit_accept_request(ListenRef, 5000) of
        {ok, {demux, Future}} ->
            RequestId = future_request_id(Future),
            _ = RequestId,
            ?TRACE_EVENT(accept_req, [{request_id, RequestId}, {timeout_ms, 5000} | ref_trace_fields(ListenRef)]),
            case await_accept_reply(Future, ListenRef, 5000) of
                {error, timeout} -> accept_loop(ListenRef);
                Result -> Result
            end;
        {ok, {local, RequestId}} ->
            ?TRACE_EVENT(accept_req, [{request_id, RequestId}, {timeout_ms, 5000} | ref_trace_fields(ListenRef)]),
            case await_runtime_reply(RequestId, self(), 5000, accept) of
                {error, timeout} -> accept_loop(ListenRef);
                {ok, Socket} = Result ->
                    register_owner_watch(Socket, self()),
                    Result;
                Result -> Result
            end;
        {error, _} = Err ->
            ?TRACE_RESULT(accept_req_error, Err, [{timeout_ms, 5000} | ref_trace_fields(ListenRef)]),
            Err
    end.

await_accept_reply(Future, ListenRef, TimeoutMs) ->
    case await_demux_reply(Future, ListenRef, TimeoutMs, accept) of
        {ok, Socket} = Result ->
            register_owner_watch(Socket, self()),
            Result;
        Result ->
            Result
    end.

-spec handshake(any(), timeout()) -> {ok, any()}.
handshake(CSocket, Timeout) ->
    handshake(CSocket, [], Timeout).

-spec handshake(any(), opts(), timeout()) -> {ok, any()}.
handshake(CSocket, _, _) ->
    {ok, CSocket}.

-spec handshake_continue(any(), timeout()) -> no_return().
handshake_continue(CSocket, Timeout) ->
    handshake_continue(CSocket, [], Timeout).

-spec handshake_continue(any(), opts(), timeout()) -> no_return().
handshake_continue(_, _, _) ->
    error(not_supported).

-spec handshake_cancel(any()) -> no_return().
handshake_cancel(_) ->
    error(not_supported).

-spec connect(inet:ip_address() | inet:hostname(), inet:port_number(), any())
    -> {ok, any()} | {error, atom()}.
connect(_Host, _Port, _Opts) ->
    {error, enotsup}.

-spec connect(inet:ip_address() | inet:hostname(), inet:port_number(), any(), timeout())
    -> {ok, any()} | {error, atom()}.
connect(_Host, _Port, _Opts, _Timeout) ->
    {error, enotsup}.

-spec recv(any(), non_neg_integer(), timeout()) -> {ok, any()} | {error, closed | atom()}.
recv(Socket, Length, Timeout) ->
    ?TRACE_EVENT(recv_req, [{len, Length}, {timeout_ms, timeout_to_ms(Timeout)} | ref_trace_fields(Socket)]),
    Result = case submit_recv_request(Socket, Length) of
        {ok, {demux, Future}} ->
            await_demux_reply(Future, Socket, Timeout, recv);
        {ok, {local, RequestId}} ->
            await_runtime_reply(RequestId, Socket, Timeout, recv);
        {error, _} = Err ->
            Err
    end,
    ?TRACE_RESULT(recv_reply, Result, [{len, Length}, {timeout_ms, timeout_to_ms(Timeout)} | ref_trace_fields(Socket)]),
    Result.

-spec recv_proxy_header(any(), timeout())
    -> {ok, ranch_proxy_header:proxy_info()}
    | {error, closed | atom()}
    | {error, protocol_error, atom()}.
recv_proxy_header(Socket, Timeout) ->
    case recv(Socket, 0, Timeout) of
        {ok, Data} ->
            case ranch_proxy_header:parse(Data) of
                {ok, ProxyInfo, <<>>} ->
                    {ok, ProxyInfo};
                {ok, _ProxyInfo, _Rest} ->
                    {error, protocol_error, leftover_proxy_bytes};
                {error, HumanReadable} ->
                    {error, protocol_error, HumanReadable}
            end;
        Error ->
            Error
    end.

-spec send(any(), iodata()) -> ok | {error, atom()}.
send(Socket, Packet) ->
    Bin = erlang:iolist_to_binary(Packet),
    ?TRACE_EVENT(send_req, [{bytes, byte_size(Bin)} | ref_trace_fields(Socket)]),
    Result = ranch_uring_nif:send_enqueue(Socket, Bin),
    ?TRACE_RESULT(send_reply, Result, [{bytes, byte_size(Bin)} | ref_trace_fields(Socket)]),
    Result.

-spec batch(any(), list()) -> {ok, list()} | {error, atom()}.
batch(Socket, Commands) ->
    batch(Socket, Commands, infinity).

-spec batch(any(), list(), timeout()) -> {ok, list()} | {error, atom()}.
batch(Socket, Commands, Timeout) ->
    Encoded = [encode_batch_command(Command) || Command <- Commands],
    TimeoutMs = timeout_to_ms(Timeout),
    case ranch_uring_nif:batch_async(Socket, Encoded) of
        {ok, RequestId} ->
            ?TRACE_EVENT(batch_req, [{request_id, RequestId}, {ops, length(Encoded)} | ref_trace_fields(Socket)]),
            await_batch_reply(RequestId, TimeoutMs, Socket);
        {error, _} = Err ->
            ?TRACE_RESULT(batch_req_error, Err, [{ops, length(Encoded)} | ref_trace_fields(Socket)]),
            Err
    end.

-spec sendv(any(), [iodata()]) -> ok | {error, atom()}.
sendv(Socket, Packets) when is_list(Packets) ->
    Bins = [erlang:iolist_to_binary(Packet) || Packet <- Packets],
    ?TRACE_EVENT(sendv_req, [{packets, length(Bins)}, {bytes, lists:sum([byte_size(Bin) || Bin <- Bins])} | ref_trace_fields(Socket)]),
    Result = ranch_uring_nif:send_enqueues(Socket, Bins),
    ?TRACE_RESULT(sendv_reply, Result, [{packets, length(Bins)}, {bytes, lists:sum([byte_size(Bin) || Bin <- Bins])} | ref_trace_fields(Socket)]),
    Result.

-spec sendfile(any(), file:name_all() | file:fd())
    -> {ok, non_neg_integer()} | {error, atom()}.
sendfile(Socket, Filename) ->
    sendfile(Socket, Filename, 0, 0, []).

-spec sendfile(any(), file:name_all() | file:fd(), non_neg_integer(), non_neg_integer())
    -> {ok, non_neg_integer()} | {error, atom()}.
sendfile(Socket, File, Offset, Bytes) ->
    sendfile(Socket, File, Offset, Bytes, []).

-spec sendfile(any(), file:name_all() | file:fd(), non_neg_integer(), non_neg_integer(),
        [{chunk_size, non_neg_integer()}])
    -> {ok, non_neg_integer()} | {error, atom()}.
sendfile(Socket, Filename, Offset, Bytes, _Opts)
        when is_list(Filename) orelse is_atom(Filename) orelse is_binary(Filename) ->
    case file:open(Filename, [read, raw, binary]) of
        {ok, RawFile} ->
            try sendfile(Socket, RawFile, Offset, Bytes, []) of
                Result -> Result
            after
                ok = file:close(RawFile)
            end;
        {error, _} = Error ->
            Error
    end;
sendfile(Socket, RawFile, Offset, Bytes, _Opts) ->
    case file:position(RawFile, Offset) of
        {ok, _} ->
            read_and_send(Socket, RawFile, Bytes, 0);
        {error, _} = Err ->
            Err
    end.

read_and_send(Socket, Fd, 0, Acc) ->
    read_and_send_eof(Socket, Fd, Acc);
read_and_send(Socket, Fd, Remaining, Acc) when Remaining > 0 ->
    ToRead = min(Remaining, 65536),
    case file:read(Fd, ToRead) of
        {ok, Data} ->
            case send(Socket, Data) of
                ok ->
                    Sent = byte_size(Data),
                    read_and_send(Socket, Fd, Remaining - Sent, Acc + Sent);
                {error, _} = Err ->
                    Err
            end;
        eof ->
            {ok, Acc};
        {error, _} = Err ->
            Err
    end.

read_and_send_eof(Socket, Fd, Acc) ->
    case file:read(Fd, 65536) of
        {ok, Data} ->
            case send(Socket, Data) of
                ok ->
                    read_and_send_eof(Socket, Fd, Acc + byte_size(Data));
                {error, _} = Err ->
                    Err
            end;
        eof ->
            {ok, Acc};
        {error, _} = Err ->
            Err
    end.

-spec setopts(any(), list()) -> ok | {error, atom()}.
setopts(Socket, Opts) ->
    case setopts_async(Socket, Opts) of
        ok ->
            LocalOpts = [Opt || {K, _} = Opt <- Opts, K =/= active, K =/= nodelay],
            case LocalOpts of
                [] -> ok;
                _ -> ranch_uring_nif:setopts(Socket, LocalOpts)
            end;
        {error, _} = Err ->
            Err
    end.

-spec getopts(any(), [atom()]) -> {ok, list()} | {error, atom()}.
getopts(Socket, Opts) ->
    ranch_uring_nif:getopts(Socket, Opts).

-spec getstat(any()) -> {ok, list()} | {error, atom()}.
getstat(_Socket) ->
    {error, enotsup}.

-spec getstat(any(), [atom()]) -> {ok, list()} | {error, atom()}.
getstat(_Socket, _OptionNames) ->
    {error, enotsup}.

-spec controlling_process(any(), pid()) -> ok | {error, closed | not_owner | atom()}.
controlling_process(Socket, Pid) ->
    PreviousWatch = take_owner_watch(Socket),
    case ranch_uring_nif:controlling_process(Socket, Pid) of
        ok ->
            replace_owner_watch(Socket, Pid, PreviousWatch),
            ok;
        Error ->
            _ = maybe_restore_owner_watch(Socket, PreviousWatch),
            Error
    end.

-spec peername(any())
    -> {ok, {inet:ip_address(), inet:port_number()} | {local, binary()}} | {error, atom()}.
peername(Socket) ->
    ranch_uring_nif:peername(Socket).

-spec sockname(any())
    -> {ok, {inet:ip_address(), inet:port_number()} | {local, binary()}} | {error, atom()}.
sockname(Socket) ->
    ranch_uring_nif:sockname(Socket).

-spec shutdown(any(), read | write | read_write) -> ok | {error, atom()}.
shutdown(Socket, How) ->
    ?TRACE_EVENT(shutdown_req, [{how, How} | ref_trace_fields(Socket)]),
    Result = ranch_uring_nif:shutdown_enqueue(Socket, How),
    ?TRACE_RESULT(shutdown_reply, Result, [{how, How} | ref_trace_fields(Socket)]),
    Result.

-spec close(any()) -> ok.
close(Socket) ->
    _ = deregister_owner_watch(Socket),
    ?TRACE_EVENT(close_req, ref_trace_fields(Socket)),
    Result = ranch_uring_nif:close_enqueue(Socket),
    ?TRACE_RESULT(close_reply, Result, ref_trace_fields(Socket)),
    Result.

register_owner_watch(Socket, OwnerPid) when is_pid(OwnerPid) ->
    ensure_owner_watch_table(),
    _ = stop_existing_owner_watch(Socket),
    Watcher = spawn_owner_watch(Socket, OwnerPid),
    true = ets:insert(?OWNER_WATCH_TABLE, {Socket, OwnerPid, Watcher}),
    ok.

replace_owner_watch(Socket, OwnerPid, PreviousWatch) when is_pid(OwnerPid) ->
    ensure_owner_watch_table(),
    Watcher = spawn_owner_watch(Socket, OwnerPid),
    true = ets:insert(?OWNER_WATCH_TABLE, {Socket, OwnerPid, Watcher}),
    stop_previous_owner_watch(PreviousWatch),
    ok.

stop_existing_owner_watch(Socket) ->
    stop_owner_watch(Socket).

stop_previous_owner_watch(undefined) ->
    ok;
stop_previous_owner_watch({_, Watcher}) ->
    Watcher ! owner_watch_stop,
    ok.

deregister_owner_watch(Socket) ->
    stop_owner_watch(Socket).

take_owner_watch(Socket) ->
    case catch ets:take(?OWNER_WATCH_TABLE, Socket) of
        [] ->
            undefined;
        [{Socket, OwnerPid, Watcher}] ->
            case Watcher =:= self() of
                true -> {OwnerPid, Watcher};
                false ->
                    Watcher ! owner_watch_stop,
                    {OwnerPid, Watcher}
            end;
        _ ->
            undefined
    end.

maybe_restore_owner_watch(_Socket, undefined) ->
    ok;
maybe_restore_owner_watch(Socket, {OwnerPid, _Watcher}) ->
    case is_process_alive(OwnerPid) of
        true ->
            register_owner_watch(Socket, OwnerPid),
            ok;
        false ->
            ok
    end.

stop_owner_watch(Socket) ->
    case take_owner_watch(Socket) of
        undefined ->
            ok;
        {_, Watcher} ->
            stop_owner_watch_pid(Watcher)
    end.

stop_owner_watch_pid(Watcher) when Watcher =:= self() ->
    ok;
stop_owner_watch_pid(Watcher) ->
    Ref = monitor(process, Watcher),
    Watcher ! owner_watch_stop,
    receive
        {'DOWN', Ref, process, Watcher, _Reason} ->
            ok
    end.

ensure_owner_watch_table() ->
    ranch_uring_owner_watch_registry:ensure_table().

spawn_owner_watch(Socket, OwnerPid) ->
    Parent = self(),
    Watcher = spawn(fun() -> owner_watch_loop(Socket, OwnerPid, Parent) end),
    receive
        {owner_watch_ready, Watcher} ->
            Watcher
    after 1000 ->
        exit({owner_watch_start_timeout, Socket, OwnerPid})
    end.

owner_watch_loop(Socket, OwnerPid, Parent) ->
    Ref = monitor(process, OwnerPid),
    Parent ! {owner_watch_ready, self()},
    receive
        {'DOWN', Ref, process, OwnerPid, _Reason} ->
            case owner_watch_should_close(Socket, self(), OwnerPid) of
                true ->
                    close_from_owner_watch(Socket);
                false ->
                    ok
            end;
        owner_watch_stop ->
            demonitor(Ref, [flush]),
            ok
    end.

close_from_owner_watch(Socket) ->
    delete_owner_watch_if_current(Socket, self()),
    ?TRACE_EVENT(close_req, ref_trace_fields(Socket)),
    case ranch_uring_nif:close(Socket) of
        Result ->
            _ = Result,
            ?TRACE_RESULT(close_reply, Result, ref_trace_fields(Socket)),
            ok
    end,
    ok.

delete_owner_watch_if_current(Socket, Watcher) ->
    case ets:lookup(?OWNER_WATCH_TABLE, Socket) of
        [{Socket, _OwnerPid, Watcher}] ->
            true = ets:delete(?OWNER_WATCH_TABLE, Socket),
            ok;
        _ ->
            ok
    end.

owner_watch_should_close(Socket, Watcher, OwnerPid) ->
    case ets:lookup(?OWNER_WATCH_TABLE, Socket) of
        [] ->
            false;
        [{Socket, OwnerPid, Watcher}] ->
            true;
        _ ->
            false
    end.

cleanup(_TransOpts) ->
    ok.

timeout_to_ms(infinity) -> -1;
timeout_to_ms(Ms) when is_integer(Ms) -> Ms.

demux_enabled() ->
    case catch ranch_uring_demux:demuxes() of
        Demuxes when is_list(Demuxes), Demuxes =/= [] -> true;
        _ -> false
    end.

submit_accept_request(ListenRef, TimeoutMs) ->
    case demux_enabled() of
        true ->
            case ranch_uring_demux:submit_accept(ListenRef, TimeoutMs) of
                {ok, Future} -> {ok, {demux, Future}};
                {error, no_demux} ->
                    submit_accept_request_local(ListenRef, TimeoutMs);
                {error, _} = Err ->
                    Err
            end;
        false ->
            submit_accept_request_local(ListenRef, TimeoutMs)
    end.

submit_accept_request_local(ListenRef, TimeoutMs) ->
    case ranch_uring_nif:accept_async(ListenRef, TimeoutMs) of
        {ok, RequestId} -> {ok, {local, RequestId}};
        {error, _} = Err -> Err
    end.

submit_recv_request(Socket, Length) ->
    case demux_enabled() of
        true ->
            case ranch_uring_demux:submit_recv(Socket, Length) of
                {ok, Future} -> {ok, {demux, Future}};
                {error, no_demux} ->
                    submit_recv_request_local(Socket, Length);
                {error, _} = Err ->
                    Err
            end;
        false ->
            submit_recv_request_local(Socket, Length)
    end.

submit_recv_request_local(Socket, Length) ->
    case ranch_uring_nif:recv_async(Socket, Length) of
        {ok, RequestId} -> {ok, {local, RequestId}};
        {error, _} = Err -> Err
    end.

setopts_async(_Socket, []) ->
    ok;
setopts_async(Socket, [{active, Value} | Rest]) ->
    ?TRACE_EVENT(set_active_req, [{value, Value} | ref_trace_fields(Socket)]),
    case ranch_uring_nif:set_active_enqueue(Socket, Value) of
        ok ->
            setopts_async(Socket, Rest);
        {error, _} = Err ->
            ?TRACE_RESULT(set_active_reply, Err, [{value, Value} | ref_trace_fields(Socket)]),
            Err
    end;
setopts_async(Socket, [{nodelay, Value} | Rest]) ->
    ?TRACE_EVENT(set_nodelay_req, [{value, Value} | ref_trace_fields(Socket)]),
    case ranch_uring_nif:set_nodelay_enqueue(Socket, Value) of
        ok ->
            setopts_async(Socket, Rest);
        {error, _} = Err ->
            ?TRACE_RESULT(set_nodelay_reply, Err, [{value, Value} | ref_trace_fields(Socket)]),
            Err
    end;
setopts_async(Socket, [_ | Rest]) ->
    setopts_async(Socket, Rest).

await_runtime_reply(RequestId, _Socket, infinity, _Op) ->
    Result = await_runtime_reply_loop(RequestId, infinity),
    ?TRACE_RUNTIME_RESULT(RequestId, Result, undefined),
    Result;
await_runtime_reply(RequestId, Socket, TimeoutMs, recv) ->
    case await_runtime_reply_loop(RequestId, TimeoutMs) of
        timeout ->
            tombstone_stashed_runtime_reply(RequestId),
            _ = ranch_uring_nif:cancel_recv(Socket),
            ?TRACE_EVENT(reply_timeout, [{request_id, RequestId} | ref_trace_fields(Socket)]),
            {error, timeout};
        Result ->
            ?TRACE_RUNTIME_RESULT(RequestId, Result, Socket),
            Result
    end;
await_runtime_reply(RequestId, _Socket, TimeoutMs, accept) ->
    case await_runtime_reply_loop(RequestId, TimeoutMs) of
        timeout ->
            tombstone_stashed_runtime_reply(RequestId),
            ?TRACE_EVENT(reply_timeout, [{request_id, RequestId}]),
            {error, timeout};
        Result ->
            ?TRACE_RUNTIME_RESULT(RequestId, Result, undefined),
            Result
    end;
await_runtime_reply(RequestId, _Socket, TimeoutMs, send) ->
    case await_runtime_reply_loop(RequestId, TimeoutMs) of
        timeout ->
            tombstone_stashed_runtime_reply(RequestId),
            ?TRACE_EVENT(reply_timeout, [{request_id, RequestId}]),
            {error, timeout};
        Result ->
            ?TRACE_RUNTIME_RESULT(RequestId, Result, undefined),
            Result
    end.

await_demux_reply({_DemuxPid, RequestId} = Future, _Socket, infinity, _Op) ->
    _ = RequestId,
    Result = ranch_uring_demux:await(Future, infinity),
    ?TRACE_RUNTIME_RESULT(RequestId, Result, undefined),
    Result;
await_demux_reply({_DemuxPid, RequestId} = Future, Socket, TimeoutMs, recv) ->
    _ = RequestId,
    case ranch_uring_demux:await(Future, TimeoutMs) of
        timeout ->
            tombstone_stashed_runtime_reply(RequestId),
            _ = ranch_uring_nif:cancel_recv(Socket),
            ?TRACE_EVENT(reply_timeout, [{request_id, RequestId} | ref_trace_fields(Socket)]),
            {error, timeout};
        Result ->
            ?TRACE_RUNTIME_RESULT(RequestId, Result, Socket),
            Result
    end;
await_demux_reply({_DemuxPid, RequestId} = Future, _Socket, TimeoutMs, accept) ->
    _ = RequestId,
    case ranch_uring_demux:await(Future, TimeoutMs) of
        timeout ->
            tombstone_stashed_runtime_reply(RequestId),
            ?TRACE_EVENT(reply_timeout, [{request_id, RequestId}]),
            {error, timeout};
        Result ->
            ?TRACE_RUNTIME_RESULT(RequestId, Result, undefined),
            Result
    end;
await_demux_reply({_DemuxPid, RequestId} = Future, _Socket, TimeoutMs, send) ->
    _ = RequestId,
    case ranch_uring_demux:await(Future, TimeoutMs) of
        timeout ->
            tombstone_stashed_runtime_reply(RequestId),
            ?TRACE_EVENT(reply_timeout, [{request_id, RequestId}]),
            {error, timeout};
        Result ->
            ?TRACE_RUNTIME_RESULT(RequestId, Result, undefined),
            Result
    end.

future_request_id({_DemuxPid, RequestId}) ->
    RequestId.

await_runtime_reply_loop(RequestId, infinity) ->
    case take_stashed_runtime_reply(RequestId) of
        {ok, Result} ->
            Result;
        error ->
            receive
                {reply, RequestId, ok} ->
                    ok;
                {reply, RequestId, {ok, Data}} ->
                    {ok, Data};
                {reply, RequestId, {error, Reason}} ->
                    {error, Reason};
                {nif_results, Pairs} ->
                    ?TRACE_EVENT(nif_results_recv,
                        [{count, length(Pairs)}, {first_id, first_pair_id(Pairs)}, {request_id, RequestId}]),
                    case stash_runtime_results_except(RequestId, Pairs) of
                        {ok, Result} ->
                            Result;
                        not_found ->
                            await_runtime_reply_loop(RequestId, infinity)
                    end
            end
    end;
await_runtime_reply_loop(RequestId, TimeoutMs) ->
    Deadline = erlang:monotonic_time(millisecond) + TimeoutMs,
    await_runtime_reply_deadline(RequestId, Deadline).

await_runtime_reply_deadline(RequestId, Deadline) ->
    case take_stashed_runtime_reply(RequestId) of
        {ok, Result} ->
            Result;
        error ->
            Remaining = Deadline - erlang:monotonic_time(millisecond),
            if
                Remaining =< 0 ->
                    timeout;
                true ->
                    receive
                        {reply, RequestId, ok} ->
                            ok;
                        {reply, RequestId, {ok, Data}} ->
                            {ok, Data};
                        {reply, RequestId, {error, Reason}} ->
                            {error, Reason};
                        {nif_results, Pairs} ->
                            ?TRACE_EVENT(nif_results_recv,
                                [{count, length(Pairs)}, {first_id, first_pair_id(Pairs)}, {request_id, RequestId}]),
                            case stash_runtime_results_except(RequestId, Pairs) of
                                {ok, Result} ->
                                    Result;
                                not_found ->
                                    await_runtime_reply_deadline(RequestId, Deadline)
                            end
                    after Remaining ->
                        timeout
                    end
            end
    end.

take_stashed_runtime_reply(RequestId) ->
    case erase({nif_result, RequestId}) of
        tombstone ->
            error;
        undefined ->
            error;
        Result ->
            {ok, Result}
    end.

tombstone_stashed_runtime_reply(RequestId) ->
    _ = put({nif_result, RequestId}, tombstone),
    ok.

stash_runtime_results_except(RequestId, Pairs) ->
    stash_runtime_results_except(RequestId, Pairs, not_found).

stash_runtime_results_except(_RequestId, [], Found) ->
    Found;
stash_runtime_results_except(RequestId, [{Id, Result} | Rest], Found0) ->
    Normalized = normalize_runtime_result(Result),
    Found = case Id =:= RequestId of
        true ->
            case Found0 of
                not_found -> {ok, Normalized};
                _ -> Found0
            end;
        false ->
            put({nif_result, Id}, Normalized),
            Found0
    end,
    stash_runtime_results_except(RequestId, Rest, Found).

normalize_runtime_result(ok) ->
    ok;
normalize_runtime_result({ok, Data}) ->
    {ok, Data};
normalize_runtime_result({error, Reason}) ->
    {error, Reason}.

first_pair_id([]) ->
    undefined;
first_pair_id([{Id, _} | _]) ->
    Id.

trace_runtime_result(RequestId, ok, _Socket) ->
    trace_event(reply_ok, [{request_id, RequestId}]);
trace_runtime_result(RequestId, {ok, Data}, _Socket) ->
    trace_reply_ok(RequestId, Data);
trace_runtime_result(RequestId, {error, Reason}, Socket) when Socket =:= undefined ->
    trace_event(reply_error, [{request_id, RequestId}, {reason, Reason}]);
trace_runtime_result(RequestId, {error, Reason}, Socket) ->
    trace_event(reply_error, [{request_id, RequestId}, {reason, Reason} | ref_trace_fields(Socket)]).

await_batch_reply(RequestId, infinity, _Socket) ->
    await_batch_reply_loop(RequestId, infinity, _Socket);
await_batch_reply(RequestId, TimeoutMs, _Socket) ->
    await_batch_reply_loop(RequestId, TimeoutMs, _Socket).

await_batch_reply_loop(RequestId, infinity, Socket) ->
    case take_stashed_batch_reply(RequestId) of
        {ok, Results} ->
            ?TRACE_EVENT(batch_reply_ok, [{request_id, RequestId}, {results, length(Results)} | ref_trace_fields(Socket)]),
            {ok, normalize_batch_results(Results)};
        error ->
            receive
                {batch_reply, RequestId, Results} ->
                    ?TRACE_EVENT(batch_reply_ok, [{request_id, RequestId}, {results, length(Results)} | ref_trace_fields(Socket)]),
                    {ok, normalize_batch_results(Results)};
                {batch_reply, OtherId, Results} ->
                    stash_batch_reply(OtherId, Results),
                    await_batch_reply_loop(RequestId, infinity, Socket)
            end
    end;
await_batch_reply_loop(RequestId, TimeoutMs, Socket) ->
    Deadline = erlang:monotonic_time(millisecond) + TimeoutMs,
    await_batch_reply_deadline(RequestId, Deadline, Socket).

await_batch_reply_deadline(RequestId, Deadline, Socket) ->
    case take_stashed_batch_reply(RequestId) of
        {ok, Results} ->
            ?TRACE_EVENT(batch_reply_ok, [{request_id, RequestId}, {results, length(Results)} | ref_trace_fields(Socket)]),
            {ok, normalize_batch_results(Results)};
        error ->
            Remaining = Deadline - erlang:monotonic_time(millisecond),
            if
                Remaining =< 0 ->
                    tombstone_stashed_batch_reply(RequestId),
                    drop_stale_batch_reply(RequestId),
                    ?TRACE_EVENT(batch_reply_timeout, [{request_id, RequestId} | ref_trace_fields(Socket)]),
                    {error, timeout};
                true ->
                    receive
                        {batch_reply, RequestId, Results} ->
                            ?TRACE_EVENT(batch_reply_ok, [{request_id, RequestId}, {results, length(Results)} | ref_trace_fields(Socket)]),
                            {ok, normalize_batch_results(Results)};
                        {batch_reply, OtherId, Results} ->
                            stash_batch_reply(OtherId, Results),
                            await_batch_reply_deadline(RequestId, Deadline, Socket)
                    after Remaining ->
                        tombstone_stashed_batch_reply(RequestId),
                        drop_stale_batch_reply(RequestId),
                        ?TRACE_EVENT(batch_reply_timeout, [{request_id, RequestId} | ref_trace_fields(Socket)]),
                        {error, timeout}
                    end
            end
    end.

take_stashed_batch_reply(RequestId) ->
    case erase({batch_reply, RequestId}) of
        tombstone ->
            error;
        undefined ->
            error;
        Results ->
            {ok, Results}
    end.

stash_batch_reply(RequestId, Results) ->
    _ = put({batch_reply, RequestId}, Results),
    ok.

tombstone_stashed_batch_reply(RequestId) ->
    _ = put({batch_reply, RequestId}, tombstone),
    ok.

drop_stale_batch_reply(RequestId) ->
    receive
        {batch_reply, RequestId, _Results} ->
            ok;
        {batch_reply, OtherId, Results} ->
            stash_batch_reply(OtherId, Results),
            drop_stale_batch_reply(RequestId)
    after 0 ->
        ok
    end.

normalize_batch_results(Results) ->
    [normalize_batch_result(Result) || Result <- Results].

normalize_batch_result({Id, ok}) ->
    {Id, ok};
normalize_batch_result({Id, {ok, Data}}) ->
    {Id, {ok, Data}};
normalize_batch_result({Id, {error, Reason}}) ->
    {Id, {error, Reason}}.

encode_batch_command({Id, writev, Data}) ->
    {Id, writev, erlang:iolist_to_binary(Data)};
encode_batch_command({Id, write, Data}) ->
    {Id, writev, erlang:iolist_to_binary(Data)};
encode_batch_command({Id, read, Length}) when is_integer(Length), Length >= 0 ->
    {Id, read, Length}.

trace_reply_ok(RequestId, Data) when is_binary(Data) ->
    trace_event(reply_ok, [{request_id, RequestId}, {bytes, byte_size(Data)}]);
trace_reply_ok(RequestId, Data) ->
    trace_result(reply_ok, {ok, Data}, [{request_id, RequestId}]).

trace_result(Name, Result, Fields) ->
    case Result of
        ok ->
            trace_event(Name, [{status, ok} | Fields]);
        {ok, Data} when is_binary(Data) ->
            trace_event(Name, [{status, ok}, {bytes, byte_size(Data)} | Fields]);
        {ok, Data} ->
            trace_event(Name, [{status, ok} | maybe_ref_trace_fields(Data, Fields)]);
        {error, Reason} ->
            trace_event(Name, [{status, error}, {reason, Reason} | Fields])
    end.

trace_event(Name, Fields) ->
    case ranch_tcp_uring_trace:enabled() of
        true ->
            ranch_tcp_uring_trace:event(Name, Fields);
        false ->
            ok
    end.

ref_trace_fields(Ref) ->
    maybe_ref_trace_fields(Ref, []).

maybe_ref_trace_fields(Ref, Fields) ->
    case ranch_tcp_uring_trace:enabled() of
        false ->
            Fields;
        true ->
            case catch ranch_uring_nif:debug_socket_id(Ref) of
                {ok, {listener, ListenerId}} ->
                    [{listener, ListenerId} | Fields];
                {ok, {session, Shard, SessionId}} ->
                    [{shard, Shard}, {session, SessionId} | Fields];
                _ ->
                    Fields
            end
    end.
