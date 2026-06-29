-module(bench_churn_split).

-export([run/0]).

-define(WORKERS, 20).
-define(REQUESTS_PER_WORKER, 300).
-define(MSG_SIZE, 512).
-define(LISTEN_BACKLOG, 1024).
-define(DEFAULT_RECV_TIMEOUT_MS, 1000).
-define(DEFAULT_COLLECT_TIMEOUT_MS, 15000).
-define(DEFAULT_ACCEPT_SUBS, 0).

run() ->
    application:ensure_all_started(ranch),
    application:ensure_all_started(ranch_uring),
    Workers = env_int("BENCH_CHURN_WORKERS", ?WORKERS),
    RequestsPerWorker = env_int("BENCH_CHURN_REQUESTS_PER_WORKER", ?REQUESTS_PER_WORKER),
    MsgSize = env_int("BENCH_CHURN_MSG_SIZE", ?MSG_SIZE),
    RecvTimeoutMs = env_int("BENCH_CHURN_RECV_TIMEOUT_MS", ?DEFAULT_RECV_TIMEOUT_MS),
    CollectTimeoutMs = env_int("BENCH_CHURN_COLLECT_TIMEOUT_MS", ?DEFAULT_COLLECT_TIMEOUT_MS),
    Payload = crypto:strong_rand_bytes(MsgSize),
    io:format("~nChurn split benchmark: ~B workers x ~B requests x ~B bytes~n~n",
              [Workers, RequestsPerWorker, MsgSize]),
    TcpConnectClose = run_connect_close(ranch_tcp, Workers, RequestsPerWorker, CollectTimeoutMs),
    UringConnectClose = run_connect_close(ranch_uring, Workers, RequestsPerWorker, CollectTimeoutMs),
    TcpPersistent = run_persistent_echo(
        ranch_tcp, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs),
    UringPersistent = run_persistent_echo(
        ranch_uring, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs),
    print_result(connect_close, ranch_tcp, TcpConnectClose),
    print_result(connect_close, ranch_uring, UringConnectClose),
    print_result(persistent_echo, ranch_tcp, TcpPersistent),
    print_result(persistent_echo, ranch_uring, UringPersistent),
    print_compare(connect_close, TcpConnectClose, UringConnectClose),
    print_compare(persistent_echo, TcpPersistent, UringPersistent),
    ok.

run_connect_close(ranch_uring, Workers, RequestsPerWorker, CollectTimeoutMs) ->
    {ok, Port, Server} = start_subscribe_accept_server(Workers, close_only),
    timer:sleep(100),
    {ElapsedUs, Latencies} = timer:tc(fun() ->
        run_connect_close_workers(Port, Workers, RequestsPerWorker, CollectTimeoutMs)
    end),
    stop_subscribe_accept_server(Server),
    format_result(Workers, RequestsPerWorker, ElapsedUs, Latencies);
run_connect_close(Transport, Workers, RequestsPerWorker, CollectTimeoutMs) ->
    Name = listener_name(Transport, "connect_close"),
    TransOpts = #{socket_opts => [{port, 0}]},
    catch ranch:stop_listener(Name),
    {ok, _} = ranch:start_listener(Name, Transport, TransOpts, bench_close_handler, []),
    Port = ranch:get_port(Name),
    timer:sleep(100),
    {ElapsedUs, Latencies} = timer:tc(fun() ->
        run_connect_close_workers(Port, Workers, RequestsPerWorker, CollectTimeoutMs)
    end),
    ranch:stop_listener(Name),
    format_result(Workers, RequestsPerWorker, ElapsedUs, Latencies).

run_persistent_echo(Transport, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs) ->
    Name = listener_name(Transport, "persistent_echo"),
    TransOpts = #{socket_opts => [{port, 0}]},
    catch ranch:stop_listener(Name),
    {ok, _} = ranch:start_listener(Name, Transport, TransOpts, bench_echo_handler, []),
    Port = ranch:get_port(Name),
    timer:sleep(100),
    {ElapsedUs, Latencies} = timer:tc(fun() ->
        run_persistent_workers(
            Port, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs)
    end),
    ranch:stop_listener(Name),
    format_result(Workers, RequestsPerWorker, ElapsedUs, Latencies).

run_connect_close_workers(Port, Workers, RequestsPerWorker, CollectTimeoutMs) ->
    Self = self(),
    Pids = [spawn_link(fun() ->
        Lats = connect_close_loop(Port, RequestsPerWorker, []),
        Self ! {done, self(), Lats}
    end) || _ <- lists:seq(1, Workers)],
    collect_latencies(Pids, [], CollectTimeoutMs).

run_persistent_workers(Port, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs) ->
    Self = self(),
    Pids = [spawn_link(fun() ->
        {ok, Sock} = connect_with_retry(Port, 100),
        Lats = persistent_echo_loop(Sock, Payload, RequestsPerWorker, RecvTimeoutMs, []),
        ok = gen_tcp:close(Sock),
        Self ! {done, self(), Lats}
    end) || _ <- lists:seq(1, Workers)],
    collect_latencies(Pids, [], CollectTimeoutMs).

connect_close_loop(_Port, 0, Acc) ->
    Acc;
connect_close_loop(Port, Remaining, Acc) ->
    T0 = erlang:monotonic_time(microsecond),
    {ok, Sock} = connect_with_retry(Port, 100),
    ok = gen_tcp:close(Sock),
    T1 = erlang:monotonic_time(microsecond),
    connect_close_loop(Port, Remaining - 1, [T1 - T0 | Acc]).

persistent_echo_loop(_Sock, _Payload, 0, _RecvTimeoutMs, Acc) ->
    Acc;
persistent_echo_loop(Sock, Payload, Remaining, RecvTimeoutMs, Acc) ->
    Size = byte_size(Payload),
    T0 = erlang:monotonic_time(microsecond),
    ok = gen_tcp:send(Sock, Payload),
    {ok, _} = recv_exact(Sock, Size, <<>>, RecvTimeoutMs),
    T1 = erlang:monotonic_time(microsecond),
    persistent_echo_loop(Sock, Payload, Remaining - 1, RecvTimeoutMs, [T1 - T0 | Acc]).

listener_name(Transport, ModeSuffix) ->
    list_to_atom(
        "bench_split_" ++ atom_to_list(Transport) ++ "_" ++ ModeSuffix ++ "_" ++
        integer_to_list(erlang:unique_integer([positive]))
    ).

start_subscribe_accept_server(Workers, SessionMode) ->
    Parent = self(),
    SubsOverride = env_int("BENCH_CHURN_ACCEPT_SUBS", ?DEFAULT_ACCEPT_SUBS),
    SubsAuto = max(2, min(Workers, 64)),
    Subs = case SubsOverride of
        N when N > 0 -> max(1, min(N, 64));
        _ -> SubsAuto
    end,
    {Pid, Ref} = spawn_monitor(fun() ->
        subscribe_accept_server(Parent, Subs, SessionMode)
    end),
    receive
        {subscribe_accept_ready, Pid, Port} ->
            {ok, Port, {Pid, Ref}};
        {subscribe_accept_error, Pid, Reason} ->
            demonitor(Ref, [flush]),
            {error, Reason};
        {'DOWN', Ref, process, Pid, Reason} ->
            {error, Reason}
    after 5000 ->
        exit(Pid, kill),
        {error, timeout}
    end.

stop_subscribe_accept_server({Pid, Ref}) ->
    Pid ! stop,
    receive
        {'DOWN', Ref, process, Pid, _Reason} ->
            ok
    after 3000 ->
        exit(Pid, kill),
        receive
            {'DOWN', Ref, process, Pid, _} ->
                ok
        after 1000 ->
            ok
        end
    end.

subscribe_accept_server(Parent, SubscriptionCount, SessionMode) ->
    case gen_tcp:listen(0, [
        binary,
        {active, false},
        {packet, raw},
        {reuseaddr, true},
        {backlog, ?LISTEN_BACKLOG}
    ]) of
        {ok, ListenSock} ->
            case prim_inet:getfd(ListenSock) of
                {ok, Fd} ->
                    case start_accept_subscriptions(Fd, SubscriptionCount) of
                        {ok, SubscriptionIds} ->
                            {ok, {_, Port}} = inet:sockname(ListenSock),
                            Parent ! {subscribe_accept_ready, self(), Port},
                            subscribe_accept_loop(SubscriptionIds, ListenSock, SessionMode);
                        Error ->
                            Parent ! {subscribe_accept_error, self(), Error},
                            gen_tcp:close(ListenSock)
                    end;
                Error ->
                    Parent ! {subscribe_accept_error, self(), Error},
                    gen_tcp:close(ListenSock)
            end;
        Error ->
            Parent ! {subscribe_accept_error, self(), Error}
    end.

start_accept_subscriptions(Fd, N) ->
    case ranch_uring_nif:command({subscribe, self(), accept, Fd}) of
        {ok, SubscriptionId} ->
            case add_accept_consumers(SubscriptionId, max(0, N - 1)) of
                ok ->
                    {ok, [SubscriptionId]};
                Error ->
                    _ = ranch_uring_nif:command({stop, SubscriptionId}),
                    Error
            end;
        Error ->
            Error
    end.

add_accept_consumers(_SubscriptionId, 0) ->
    ok;
add_accept_consumers(SubscriptionId, N) ->
    case ranch_uring_nif:command({subscribe, new_consumer, SubscriptionId}) of
        ok ->
            add_accept_consumers(SubscriptionId, N - 1);
        Error ->
            Error
    end.

subscribe_accept_loop(SubscriptionIds, ListenSock, SessionMode) ->
    receive
        stop ->
            lists:foreach(
                fun(Id) ->
                    _ = ranch_uring_nif:command({stop, Id})
                end,
                SubscriptionIds
            ),
            gen_tcp:close(ListenSock),
            ok;
        {reply, RequestId, Reply} when is_integer(RequestId) ->
            handle_accept_reply(SubscriptionIds, RequestId, Reply, SessionMode),
            subscribe_accept_loop(SubscriptionIds, ListenSock, SessionMode);
        {nif_results, Pairs} ->
            lists:foreach(
                fun
                    ({RequestId, Reply}) when is_integer(RequestId) ->
                        handle_accept_reply(SubscriptionIds, RequestId, Reply, SessionMode);
                    (_) ->
                        ok
                end,
                Pairs
            ),
            subscribe_accept_loop(SubscriptionIds, ListenSock, SessionMode);
        _Other ->
            subscribe_accept_loop(SubscriptionIds, ListenSock, SessionMode)
    end.

handle_accept_reply(SubscriptionIds, RequestId, {ok, Session}, SessionMode) ->
    case lists:member(RequestId, SubscriptionIds) of
        true ->
            spawn(fun() -> handle_session(Session, SessionMode) end);
        false ->
            ok
    end;
handle_accept_reply(SubscriptionIds, RequestId, {error, _Reason}, _SessionMode) ->
    case lists:member(RequestId, SubscriptionIds) of
        true -> ok;
        false -> ok
    end;
handle_accept_reply(_SubscriptionIds, _RequestId, _Reply, _SessionMode) ->
    ok.

handle_session(Session, close_only) ->
    _ = ranch_uring_nif:close_enqueue(Session),
    ok;
handle_session(Session, echo_once) ->
    case ranch_uring_nif:recv(Session, 0, 5000) of
        {ok, Data} when is_binary(Data) ->
            _ = ranch_uring_nif:send(Session, Data),
            ok;
        _ ->
            ok
    end.

connect_with_retry(Port, AttemptsLeft) ->
    case gen_tcp:connect({127,0,0,1}, Port, [binary, {active, false}, {packet, raw}]) of
        {ok, Sock} ->
            {ok, Sock};
        {error, eaddrnotavail} when AttemptsLeft > 0 ->
            timer:sleep(5),
            connect_with_retry(Port, AttemptsLeft - 1);
        {error, eaddrinuse} when AttemptsLeft > 0 ->
            timer:sleep(5),
            connect_with_retry(Port, AttemptsLeft - 1);
        {error, Reason} ->
            error({connect_failed, Reason})
    end.

recv_exact(_Sock, 0, Acc, _RecvTimeoutMs) ->
    {ok, Acc};
recv_exact(Sock, Remaining, Acc, RecvTimeoutMs) ->
    case gen_tcp:recv(Sock, 0, RecvTimeoutMs) of
        {ok, Data} ->
            Got = byte_size(Data),
            recv_exact(Sock, Remaining - Got, <<Acc/binary, Data/binary>>, RecvTimeoutMs);
        {error, _} = Err ->
            Err
    end.

collect_latencies([], Acc, _CollectTimeoutMs) ->
    lists:sort(lists:flatten(Acc));
collect_latencies(Pids, Acc, CollectTimeoutMs) ->
    receive
        {done, Pid, Lats} ->
            collect_latencies(lists:delete(Pid, Pids), [Lats | Acc], CollectTimeoutMs)
    after CollectTimeoutMs ->
        error(churn_split_timeout)
    end.

format_result(Workers, RequestsPerWorker, ElapsedUs, Latencies) ->
    TotalRequests = Workers * RequestsPerWorker,
    #{
        elapsed_us => ElapsedUs,
        total_requests => TotalRequests,
        req_per_sec => TotalRequests * 1000000 / ElapsedUs,
        latencies_us => Latencies,
        p50 => percentile(Latencies, 50),
        p99 => percentile(Latencies, 99),
        p999 => percentile(Latencies, 99.9)
    }.

env_int(Name, Default) ->
    case os:getenv(Name) of
        false -> Default;
        [] -> Default;
        Value ->
            case string:to_integer(Value) of
                {Int, []} when Int >= 0 -> Int;
                _ -> Default
            end
    end.

percentile(Sorted, P) ->
    Len = length(Sorted),
    Idx = max(1, min(Len, round(P / 100 * Len))),
    lists:nth(Idx, Sorted).

print_result(Mode, Transport, Result) ->
    io:format("=== ~p / ~p ===~n", [Mode, Transport]),
    io:format("  Requests:            ~B~n", [maps:get(total_requests, Result)]),
    io:format("  Throughput:          ~w req/s~n", [trunc(maps:get(req_per_sec, Result))]),
    io:format("  Latency p50:         ~w us~n", [maps:get(p50, Result)]),
    io:format("  Latency p99:         ~w us~n", [maps:get(p99, Result)]),
    io:format("  Latency p99.9:       ~w us~n~n", [maps:get(p999, Result)]).

print_compare(Mode, TcpResult, UringResult) ->
    TcpRps = maps:get(req_per_sec, TcpResult),
    UringRps = maps:get(req_per_sec, UringResult),
    Diff = (UringRps - TcpRps) / TcpRps * 100,
    Sign = case Diff >= 0 of true -> "+"; false -> "" end,
    io:format("=== Comparison (~p) ===~n", [Mode]),
    io:format("  ranch_uring vs ranch_tcp: ~s~.1f% throughput~n", [Sign, Diff]),
    io:format("  p50 latency: ~w us vs ~w us~n",
              [maps:get(p50, UringResult), maps:get(p50, TcpResult)]),
    io:format("  p99 latency: ~w us vs ~w us~n~n",
              [maps:get(p99, UringResult), maps:get(p99, TcpResult)]).
