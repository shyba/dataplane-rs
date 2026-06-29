-module(bench_stop_convergence).

-export([run/0]).

-define(DEFAULT_WORKERS, 24).
-define(DEFAULT_REQUESTS_PER_WORKER, 200).
-define(DEFAULT_MSG_SIZE, 64).
-define(DEFAULT_ACCEPT_SUBSCRIPTIONS, 4).
-define(DEFAULT_TRIGGER_AFTER_MS, 120).
-define(DEFAULT_DRAIN_MS, 500).
-define(LISTEN_BACKLOG, 1024).
-define(DEFAULT_CONNECT_TIMEOUT_MS, 500).
-define(DEFAULT_RECV_TIMEOUT_MS, 2000).
-define(DEFAULT_CLIENT_COLLECT_TIMEOUT_MS, 8000).
-define(DEFAULT_SERVER_WAIT_MS, 4000).

run() ->
    application:ensure_all_started(ranch),
    application:ensure_all_started(ranch_uring),

    Workers = env_int("BENCH_STOP_WORKERS", ?DEFAULT_WORKERS),
    RequestsPerWorker = env_int("BENCH_STOP_REQUESTS_PER_WORKER", ?DEFAULT_REQUESTS_PER_WORKER),
    MsgSize = env_int("BENCH_STOP_MSG_SIZE", ?DEFAULT_MSG_SIZE),
    AcceptSubs = max(1, env_int("BENCH_STOP_ACCEPT_SUBS", ?DEFAULT_ACCEPT_SUBSCRIPTIONS)),
    TriggerAfterMs = max(0, env_int("BENCH_STOP_TRIGGER_AFTER_MS", ?DEFAULT_TRIGGER_AFTER_MS)),
    DrainMs = max(1, env_int("BENCH_STOP_DRAIN_MS", ?DEFAULT_DRAIN_MS)),
    ClientCollectTimeoutMs = max(1, env_int(
        "BENCH_STOP_CLIENT_COLLECT_MS", ?DEFAULT_CLIENT_COLLECT_TIMEOUT_MS
    )),
    Payload = crypto:strong_rand_bytes(max(1, MsgSize)),

    io:format(
        "~nStop-convergence bench (~B workers x ~B requests/worker x ~B bytes)~n"
        "  accept_subscriptions: ~B~n  trigger_after_ms: ~B~n  drain_ms: ~B~n~n",
        [Workers, RequestsPerWorker, byte_size(Payload), AcceptSubs, TriggerAfterMs, DrainMs]
    ),

    Self = self(),
    {ok, Port, {ServerPid, ServerRef}} = start_subscribe_server(Self, Payload, AcceptSubs, DrainMs),
    _TriggerRef = timer:send_after(TriggerAfterMs, ServerPid, trigger_stop),

    ClientResult = collect_client_results(
        run_client_workers(Port, Payload, Workers, RequestsPerWorker),
        ClientCollectTimeoutMs,
        []
    ),
    ServerResult = collect_server_result(
        ServerPid,
        max(ClientCollectTimeoutMs, DrainMs + ?DEFAULT_SERVER_WAIT_MS)
    ),

    demonitor(ServerRef, [flush]),

    io:format(
        "client result: attempts=~B success=~B connect_err=~B send_err=~B recv_err=~B "
        "max_lat_us=~B avg_lat_us=~.2f~n",
        [
            maps:get(attempts, ClientResult),
            maps:get(success, ClientResult),
            maps:get(connect_errors, ClientResult),
            maps:get(send_errors, ClientResult),
            maps:get(recv_errors, ClientResult),
            maps:get(max_latency_us, ClientResult),
            maps:get(avg_latency_us, ClientResult)
        ]
    ),
    io:format(
        "server result: accepted_before_stop=~B accepted_after_stop=~B stop_ns=~B drain_ms=~B~n~n",
        [
            maps:get(accepted_before_stop, ServerResult),
            maps:get(accepted_after_stop, ServerResult),
            maps:get(stop_requested_ns, ServerResult),
            maps:get(drain_ms, ServerResult)
        ]
    ),
    ok.

run_client_workers(Port, Payload, Workers, RequestsPerWorker) ->
    Self = self(),
    [spawn_link(fun() ->
        Self ! {client_done, self(), oneshot_client_loop(Port, Payload, RequestsPerWorker)}
    end) || _ <- lists:seq(1, Workers)].

oneshot_client_loop(Port, Payload, RequestsPerWorker) ->
    oneshot_client_loop(Port, Payload, RequestsPerWorker, 0, 0, 0, 0, 0, 0).

start_subscribe_server(Parent, Payload, SubscriptionCount, DrainMs) ->
    {Pid, Ref} = spawn_monitor(fun() ->
        subscribe_server_main(Parent, Payload, SubscriptionCount, DrainMs)
    end),
    receive
        {subscribe_server_ready, Pid, Port} ->
            {ok, Port, {Pid, Ref}};
        {subscribe_server_error, Pid, Reason} ->
            demonitor(Ref, [flush]),
            exit({start_subscribe_server_failed, Reason});
        {'DOWN', Ref, process, Pid, Reason} ->
            exit({start_subscribe_server_exit, Reason})
    after 5000 ->
        demonitor(Ref, [flush]),
        exit(start_subscribe_server_timeout)
    end.

subscribe_server_main(Parent, Payload, SubscriptionCount, DrainMs) ->
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
                    case start_accept_subscriptions(Fd, max(1, SubscriptionCount)) of
                        {ok, SubscriptionIds} ->
                            {ok, {_, Port}} = inet:sockname(ListenSock),
                            Parent ! {subscribe_server_ready, self(), Port},
                            SubscriptionSet = maps:from_list([{Id, true} || Id <- SubscriptionIds]),
                            subscribe_server_loop(
                                Parent,
                                ListenSock,
                                SubscriptionIds,
                                SubscriptionSet,
                                Payload,
                                DrainMs,
                                0,
                                0,
                                0,
                                false,
                                undefined,
                                false
                            );
                        Error ->
                            gen_tcp:close(ListenSock),
                            Parent ! {subscribe_server_error, self(), Error}
                    end;
                Error ->
                    gen_tcp:close(ListenSock),
                    Parent ! {subscribe_server_error, self(), Error}
            end;
        Error ->
            Parent ! {subscribe_server_error, self(), Error}
    end.

subscribe_server_loop(
    Parent,
    ListenSock,
    SubscriptionIds,
    SubscriptionSet,
    Payload,
    DrainMs,
    StopTimeNs,
    AcceptedBeforeStop,
    AcceptedAfterStop,
    Stopping,
    _DrainRef,
    ListenClosed
) ->
    receive
        trigger_stop ->
            case Stopping of
                false ->
                    initiate_stop(
                        Parent,
                        ListenSock,
                        SubscriptionIds,
                        SubscriptionSet,
                        Payload,
                        DrainMs,
                        StopTimeNs,
                        AcceptedBeforeStop,
                        AcceptedAfterStop
                    );
                true ->
                    subscribe_server_loop(
                        Parent,
                        ListenSock,
                        SubscriptionIds,
                        SubscriptionSet,
                        Payload,
                        DrainMs,
                        StopTimeNs,
                        AcceptedBeforeStop,
                        AcceptedAfterStop,
                        Stopping,
                        undefined,
                        ListenClosed
                    )
            end;
        force_stop ->
            case Stopping of
                false ->
                    initiate_stop(
                        Parent,
                        ListenSock,
                        SubscriptionIds,
                        SubscriptionSet,
                        Payload,
                        DrainMs,
                        StopTimeNs,
                        AcceptedBeforeStop,
                        AcceptedAfterStop
                    );
                true ->
                    subscribe_server_loop(
                        Parent,
                        ListenSock,
                        SubscriptionIds,
                        SubscriptionSet,
                        Payload,
                        DrainMs,
                        StopTimeNs,
                        AcceptedBeforeStop,
                        AcceptedAfterStop,
                        Stopping,
                        undefined,
                        ListenClosed
                    )
            end;
        {reply, RequestId, Reply} when is_integer(RequestId) ->
            {NewBefore, NewAfter} = handle_reply(
                Reply,
                RequestId,
                SubscriptionSet,
                Payload,
                Stopping,
                AcceptedBeforeStop,
                AcceptedAfterStop
            ),
            subscribe_server_loop(
                Parent,
                ListenSock,
                SubscriptionIds,
                SubscriptionSet,
                Payload,
                DrainMs,
                StopTimeNs,
                NewBefore,
                NewAfter,
                Stopping,
                undefined,
                ListenClosed
            );
        {nif_results, Pairs} when is_list(Pairs) ->
            {NewBefore, NewAfter} = lists:foldl(
                fun
                    ({RequestId, Reply}, {AccBefore, AccAfter}) ->
                        handle_reply(
                            Reply,
                            RequestId,
                            SubscriptionSet,
                            Payload,
                            Stopping,
                            AccBefore,
                            AccAfter
                        );
                    (_, Acc) ->
                        Acc
                end,
                {AcceptedBeforeStop, AcceptedAfterStop},
                Pairs
            ),
            subscribe_server_loop(
                Parent,
                ListenSock,
                SubscriptionIds,
                SubscriptionSet,
                Payload,
                DrainMs,
                StopTimeNs,
                NewBefore,
                NewAfter,
                Stopping,
                undefined,
                ListenClosed
            );
        drain_timeout when Stopping ->
            Parent ! {subscribe_server_done, self(), #{
                accepted_before_stop => AcceptedBeforeStop,
                accepted_after_stop => AcceptedAfterStop,
                stop_requested_ns => StopTimeNs,
                drain_ms => DrainMs
            }},
            exit(normal);
        _Other ->
            subscribe_server_loop(
                Parent,
                ListenSock,
                SubscriptionIds,
                SubscriptionSet,
                Payload,
                DrainMs,
                StopTimeNs,
                AcceptedBeforeStop,
                AcceptedAfterStop,
                Stopping,
                undefined,
                ListenClosed
            )
    end.

handle_reply(
    Reply,
    RequestId,
    SubscriptionSet,
    Payload,
    Stopping,
    AcceptedBeforeStop,
    AcceptedAfterStop
) ->
    _ = RequestId,
    case maps:is_key(RequestId, SubscriptionSet) of
        false ->
            {AcceptedBeforeStop, AcceptedAfterStop};
        true ->
            case Reply of
                {ok, Session} ->
                    _ = spawn_link(fun() -> echo_once_session(Session, Payload) end),
                    case Stopping of
                        true ->
                            {AcceptedBeforeStop, AcceptedAfterStop + 1};
                        false ->
                            {AcceptedBeforeStop + 1, AcceptedAfterStop}
                    end;
                _ ->
                    {AcceptedBeforeStop, AcceptedAfterStop}
            end
    end.

initiate_stop(
    Parent,
    ListenSock,
    SubscriptionIds,
    _SubscriptionSet,
    Payload,
    DrainMs,
    ExistingStopTimeNs,
    AcceptedBeforeStop,
    AcceptedAfterStop
) ->
    NewStopTimeNs = case ExistingStopTimeNs of
        0 -> now_ns();
        _ -> ExistingStopTimeNs
    end,
    Parent ! {subscribe_server_stopping, self(), NewStopTimeNs},
    lists:foreach(
        fun(Id) -> _ = ranch_uring_nif:command({stop, Id}) end,
        SubscriptionIds
    ),
    close_if_open(ListenSock, false),
    _ = timer:send_after(DrainMs, self(), drain_timeout),
    subscribe_server_loop(
        Parent,
        ListenSock,
        SubscriptionIds,
        #{},
        Payload,
        DrainMs,
        NewStopTimeNs,
        AcceptedBeforeStop,
        AcceptedAfterStop,
        true,
        undefined,
        true
    ).

echo_once_session(Session, Payload) ->
    Size = byte_size(Payload),
    case ranch_uring_nif:recv(Session, Size, ?DEFAULT_RECV_TIMEOUT_MS) of
        {ok, Data} when is_binary(Data), byte_size(Data) =:= Size ->
            _ = ranch_uring_nif:send(Session, Data),
            _ = ranch_uring_nif:close(Session);
        _ ->
            _ = ranch_uring_nif:close(Session)
    end.

close_if_open(_ListenSock, true) ->
    ok;
close_if_open(ListenSock, false) ->
    catch gen_tcp:close(ListenSock),
    ok.

start_accept_subscriptions(_Fd, 0, Acc) ->
    {ok, lists:reverse(Acc)};
start_accept_subscriptions(Fd, N, Acc) ->
    case ranch_uring_nif:command({subscribe, self(), accept, Fd}) of
        {ok, SubscriptionId} ->
            case add_accept_consumers(SubscriptionId, N - 1) of
                ok ->
                    start_accept_subscriptions(Fd, N - 1, [SubscriptionId | Acc]);
                Error ->
                    lists:foreach(
                        fun(Id) -> _ = ranch_uring_nif:command({stop, Id}) end,
                        [SubscriptionId | Acc]
                    ),
                    Error
            end;
        Error ->
            Error
    end.

start_accept_subscriptions(Fd, N) ->
    start_accept_subscriptions(Fd, max(1, N), []).

add_accept_consumers(_SubscriptionId, 0) ->
    ok;
add_accept_consumers(SubscriptionId, N) ->
    case ranch_uring_nif:command({subscribe, new_consumer, SubscriptionId}) of
        ok ->
            add_accept_consumers(SubscriptionId, N - 1);
        Error ->
            Error
    end.

oneshot_client_loop(
    _Port,
    _Payload,
    0,
    Success,
    ConnectErrors,
    SendErrors,
    RecvErrors,
    LatencySumUs,
    MaxLatencyUs
) ->
    Attempts = Success + ConnectErrors + SendErrors + RecvErrors,
    AvgLatencyUs = case Success of
        0 -> 0.0;
        _ -> LatencySumUs / Success
    end,
    #{
        attempts => Attempts,
        success => Success,
        connect_errors => ConnectErrors,
        send_errors => SendErrors,
        recv_errors => RecvErrors,
        stop_errors => 0,
        avg_latency_us => AvgLatencyUs,
        max_latency_us => MaxLatencyUs,
        latency_sum_us => LatencySumUs
    };
oneshot_client_loop(
    Port,
    Payload,
    Remaining,
    Success,
    ConnectErrors,
    SendErrors,
    RecvErrors,
    LatencySumUs,
    MaxLatencyUs
) ->
    Size = byte_size(Payload),
    case gen_tcp:connect(
        {127, 0, 0, 1},
        Port,
        [binary, {active, false}, {packet, raw}],
        ?DEFAULT_CONNECT_TIMEOUT_MS
    ) of
        {ok, Sock} ->
            T0 = erlang:monotonic_time(microsecond),
            case gen_tcp:send(Sock, Payload) of
                ok ->
                    case recv_exact(Sock, Size, <<>>) of
                        {ok, _} ->
                            _ = gen_tcp:close(Sock),
                            T1 = erlang:monotonic_time(microsecond),
                            LatencyUs = T1 - T0,
                            oneshot_client_loop(
                                Port,
                                Payload,
                                Remaining - 1,
                                Success + 1,
                                ConnectErrors,
                                SendErrors,
                                RecvErrors,
                                LatencySumUs + LatencyUs,
                                max(MaxLatencyUs, LatencyUs)
                            );
                        {error, _} ->
                            _ = gen_tcp:close(Sock),
                            oneshot_client_loop(
                                Port,
                                Payload,
                                Remaining - 1,
                                Success,
                                ConnectErrors,
                                SendErrors,
                                RecvErrors + 1,
                                LatencySumUs,
                                MaxLatencyUs
                            )
                    end;
                {error, _} ->
                    _ = gen_tcp:close(Sock),
                    oneshot_client_loop(
                        Port,
                        Payload,
                        Remaining - 1,
                        Success,
                        ConnectErrors,
                        SendErrors + 1,
                        RecvErrors,
                        LatencySumUs,
                        MaxLatencyUs
                    )
            end;
        {error, _} ->
            oneshot_client_loop(
                Port,
                Payload,
                Remaining - 1,
                Success,
                ConnectErrors + 1,
                SendErrors,
                RecvErrors,
                LatencySumUs,
                MaxLatencyUs
            )
    end.

collect_client_results([], _CollectTimeoutMs, Acc) ->
    fold_client_stats(Acc);
collect_client_results(Pids, CollectTimeoutMs, Acc) ->
    receive
        {client_done, Pid, Stats} ->
            collect_client_results(
                lists:delete(Pid, Pids),
                CollectTimeoutMs,
                [Stats | Acc]
            );
        _Other ->
            collect_client_results(Pids, CollectTimeoutMs, Acc)
    after CollectTimeoutMs ->
        error(bench_stop_clients_timeout)
    end.

collect_server_result(ServerPid, TimeoutMs) ->
    MonitorRef = monitor(process, ServerPid),
    ServerPid ! force_stop,
    receive
        {subscribe_server_done, ServerPid, Stats} ->
            demonitor(MonitorRef, [flush]),
            Stats;
        {'DOWN', MonitorRef, process, ServerPid, _Reason} ->
            error(bench_stop_server_gone)
    after TimeoutMs ->
        erlang:demonitor(MonitorRef, [flush]),
        error(bench_stop_server_timeout)
    end.

fold_client_stats(StatsList) ->
    Folded = lists:foldl(
        fun(S, Acc) ->
            Attempts = maps:get(attempts, Acc) + maps:get(attempts, S, 0),
            Success = maps:get(success, Acc) + maps:get(success, S, 0),
            ConnectErrors = maps:get(connect_errors, Acc) + maps:get(connect_errors, S, 0),
            SendErrors = maps:get(send_errors, Acc) + maps:get(send_errors, S, 0),
            RecvErrors = maps:get(recv_errors, Acc) + maps:get(recv_errors, S, 0),
            StopErrors = maps:get(stop_errors, Acc) + maps:get(stop_errors, S, 0),
            LatencySum = maps:get(latency_sum_us, Acc) + maps:get(latency_sum_us, S, 0),
            MaxLatency = max(maps:get(max_latency_us, Acc), maps:get(max_latency_us, S, 0)),
            #{
                attempts => Attempts,
                success => Success,
                connect_errors => ConnectErrors,
                send_errors => SendErrors,
                recv_errors => RecvErrors,
                stop_errors => StopErrors,
                latency_sum_us => LatencySum,
                max_latency_us => MaxLatency
            }
        end,
        #{
            attempts => 0,
            success => 0,
            connect_errors => 0,
            send_errors => 0,
            recv_errors => 0,
            stop_errors => 0,
            latency_sum_us => 0,
            max_latency_us => 0
        },
        StatsList
    ),
    AvgLatencyUs = case maps:get(success, Folded) of
        0 -> 0.0;
        SuccessCount -> maps:get(latency_sum_us, Folded) / SuccessCount
    end,
    maps:put(avg_latency_us, AvgLatencyUs, Folded).

recv_exact(_Sock, 0, Acc) ->
    {ok, Acc};
recv_exact(Sock, Remaining, Acc) ->
    case gen_tcp:recv(Sock, 0, ?DEFAULT_RECV_TIMEOUT_MS) of
        {ok, Data} ->
            New = <<Acc/binary, Data/binary>>,
            case byte_size(New) >= Remaining of
                true ->
                    {ok, New};
                false ->
                    recv_exact(Sock, Remaining - byte_size(Data), New)
            end;
        {error, _} = Err ->
            Err
    end.

now_ns() ->
    erlang:monotonic_time(nanosecond).

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
