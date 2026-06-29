-module(bench_modes).

-export([run/0]).

-define(CLIENTS, 50).
-define(MESSAGES_PER_CLIENT, 1000).
-define(MSG_SIZE, 512).

run() ->
    application:ensure_all_started(ranch),
    application:ensure_all_started(ranch_uring),
    Payload = crypto:strong_rand_bytes(?MSG_SIZE),
    io:format("~nMode benchmark: ~B clients x ~B messages x ~B bytes~n~n",
              [?CLIENTS, ?MESSAGES_PER_CLIENT, ?MSG_SIZE]),
    Results = [
        {ranch_tcp, passive, run_transport(ranch_tcp, passive, Payload)},
        {ranch_tcp, sendv, run_transport(ranch_tcp, sendv, Payload)},
        {ranch_uring, passive, run_transport(ranch_uring, passive, Payload)},
        {ranch_uring, batch, run_transport(ranch_uring, batch, Payload)},
        {ranch_tcp_uring, batch, run_transport(ranch_tcp_uring, batch, Payload)},
        {ranch_tcp_uring, sendv, run_transport(ranch_tcp_uring, sendv, Payload)},
        {ranch_tcp, active, run_transport(ranch_tcp, active, Payload)},
        {ranch_uring, active, run_transport(ranch_uring, active, Payload)}
    ],
    lists:foreach(fun print_result/1, Results),
    ok.

run_transport(Transport, Mode, Payload) ->
    {Clients, MessagesPerClient} = workload_for_mode(Mode),
    Name = list_to_atom("bench_" ++ atom_to_list(Transport) ++ "_" ++ atom_to_list(Mode)),
    TransOpts = #{socket_opts => [{port, 0}]},
    {Handler, ProtoOpts} = protocol_for_mode(Mode, Payload),
    {ok, _} = ranch:start_listener(Name, Transport, TransOpts, Handler, ProtoOpts),
    Port = ranch:get_port(Name),
    timer:sleep(100),
    {ConnTime, _} = timer:tc(fun() -> bench_connect(Port, Clients) end),
    {ThroughputTime, Latencies} = timer:tc(fun() ->
        bench_echo(Port, Payload, Clients, MessagesPerClient)
    end),
    ranch:stop_listener(Name),
    TotalMessages = Clients * MessagesPerClient,
    TotalBytes = TotalMessages * byte_size(Payload) * 2,
    #{
        clients => Clients,
        messages_per_client => MessagesPerClient,
        connect_time_us => ConnTime,
        throughput_time_us => ThroughputTime,
        total_messages => TotalMessages,
        total_bytes => TotalBytes,
        msg_per_sec => TotalMessages * 1000000 / ThroughputTime,
        mb_per_sec => TotalBytes / ThroughputTime,
        latencies_us => Latencies,
        p50 => percentile(Latencies, 50),
        p99 => percentile(Latencies, 99),
        p999 => percentile(Latencies, 99.9)
    }.

protocol_for_mode(passive, _Payload) ->
    {bench_echo_handler, []};
protocol_for_mode(sendv, _Payload) ->
    {bench_echo_modes_handler, [{mode, sendv}]};
protocol_for_mode(active, _Payload) ->
    {bench_active, []};
protocol_for_mode(batch, Payload) ->
    {bench_echo_modes_handler, [{mode, batch}, {payload, Payload}]}.

workload_for_mode(batch) ->
    {10, 100};
workload_for_mode(_) ->
    {?CLIENTS, ?MESSAGES_PER_CLIENT}.

bench_connect(Port, N) ->
    Self = self(),
    Pids = [spawn_link(fun() ->
        {ok, Sock} = gen_tcp:connect({127,0,0,1}, Port,
                                     [binary, {active, false}, {packet, raw}]),
        gen_tcp:close(Sock),
        Self ! {done, self()}
    end) || _ <- lists:seq(1, N)],
    wait_all(Pids).

bench_echo(Port, Payload, NumClients, MsgsPerClient) ->
    Self = self(),
    Pids = [spawn_link(fun() ->
        {ok, Sock} = gen_tcp:connect({127,0,0,1}, Port,
                                     [binary, {active, false}, {packet, raw}]),
        Lats = echo_loop(Sock, Payload, MsgsPerClient, []),
        gen_tcp:close(Sock),
        Self ! {done, self(), Lats}
    end) || _ <- lists:seq(1, NumClients)],
    collect_latencies(Pids, []).

echo_loop(_Sock, _Payload, 0, Acc) ->
    Acc;
echo_loop(Sock, Payload, N, Acc) ->
    Size = byte_size(Payload),
    T0 = erlang:monotonic_time(microsecond),
    ok = gen_tcp:send(Sock, Payload),
    {ok, _} = recv_exact(Sock, Size, <<>>),
    T1 = erlang:monotonic_time(microsecond),
    echo_loop(Sock, Payload, N - 1, [T1 - T0 | Acc]).

recv_exact(_Sock, 0, Acc) ->
    {ok, Acc};
recv_exact(Sock, Remaining, Acc) ->
    case gen_tcp:recv(Sock, 0, 30000) of
        {ok, Data} ->
            Got = byte_size(Data),
            recv_exact(Sock, Remaining - Got, <<Acc/binary, Data/binary>>);
        {error, _} = Err ->
            Err
    end.

collect_latencies([], Acc) ->
    lists:sort(lists:flatten(Acc));
collect_latencies(Pids, Acc) ->
    receive
        {done, Pid, Lats} ->
            collect_latencies(lists:delete(Pid, Pids), [Lats | Acc])
    after 30000 ->
        error(bench_timeout)
    end.

wait_all([]) -> ok;
wait_all(Pids) ->
    receive
        {done, Pid} -> wait_all(lists:delete(Pid, Pids))
    after 10000 ->
        error(connect_timeout)
    end.

percentile(Sorted, P) ->
    Len = length(Sorted),
    Idx = max(1, min(Len, round(P / 100 * Len))),
    lists:nth(Idx, Sorted).

print_result({Transport, Mode, R}) ->
    io:format("=== ~s/~s ===~n", [Transport, Mode]),
    io:format("  Workload:            ~B clients x ~B messages~n",
              [maps:get(clients, R), maps:get(messages_per_client, R)]),
    io:format("  Connect ~B clients:  ~.1f ms~n",
              [maps:get(clients, R), maps:get(connect_time_us, R) / 1000]),
    io:format("  Throughput:          ~w msg/s~n", [trunc(maps:get(msg_per_sec, R))]),
    io:format("  Bandwidth:           ~.1f MB/s~n", [maps:get(mb_per_sec, R)]),
    io:format("  Latency p50:         ~w us~n", [maps:get(p50, R)]),
    io:format("  Latency p99:         ~w us~n", [maps:get(p99, R)]),
    io:format("  Latency p99.9:       ~w us~n~n", [maps:get(p999, R)]).
