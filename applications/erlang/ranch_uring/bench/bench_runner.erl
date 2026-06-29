-module(bench_runner).

-export([run/0, run_mqd/0]).

-define(CLIENTS, 50).
-define(MESSAGES_PER_CLIENT, 1000).
-define(MSG_SIZE, 512).

run() ->
    application:ensure_all_started(ranch),
    application:ensure_all_started(ranch_uring),
    {module, ranch_tcp_uring} = code:ensure_loaded(ranch_tcp_uring),
    Payload = crypto:strong_rand_bytes(?MSG_SIZE),
    io:format("~nBenchmark: ~B clients x ~B messages x ~B bytes~n~n",
              [?CLIENTS, ?MESSAGES_PER_CLIENT, ?MSG_SIZE]),
    TcpResult = run_transport(ranch_tcp, Payload),
    TcpUringResult = run_transport(ranch_tcp_uring, Payload),
    UringResult = run_transport(ranch_uring, Payload),
    print_results(ranch_tcp, TcpResult),
    print_results(ranch_tcp_uring, TcpUringResult),
    print_results(ranch_uring, UringResult),
    print_comparison(ranch_tcp_uring, TcpResult, TcpUringResult),
    print_comparison(ranch_uring, TcpResult, UringResult),
    ok.

run_mqd() ->
    application:ensure_all_started(ranch),
    application:ensure_all_started(ranch_uring),
    {module, ranch_tcp_uring} = code:ensure_loaded(ranch_tcp_uring),
    Payload = crypto:strong_rand_bytes(?MSG_SIZE),
    io:format("~nMQD benchmark: ~B clients x ~B messages x ~B bytes~n~n",
              [?CLIENTS, ?MESSAGES_PER_CLIENT, ?MSG_SIZE]),
    lists:foreach(
        fun({Transport, MQD}) ->
            Result = run_transport(Transport, Payload, MQD),
            io:format("=== ~s/~p ===~n", [Transport, MQD]),
            io:format("  Throughput:          ~w msg/s~n", [trunc(maps:get(msg_per_sec, Result))]),
            io:format("  Latency p50:         ~w us~n", [maps:get(p50, Result)]),
            io:format("  Latency p99:         ~w us~n", [maps:get(p99, Result)]),
            io:format("  Latency p99.9:       ~w us~n~n", [maps:get(p999, Result)])
        end,
        [
            {ranch_tcp, on_heap},
            {ranch_tcp, off_heap},
            {ranch_tcp_uring, on_heap},
            {ranch_tcp_uring, off_heap},
            {ranch_uring, on_heap},
            {ranch_uring, off_heap}
        ]),
    ok.

run_transport(Transport, Payload) ->
    run_transport(Transport, Payload, on_heap).

run_transport(Transport, Payload, MQD) ->
    Name = list_to_atom("bench_" ++ atom_to_list(Transport)),
    TransOpts = #{socket_opts => [{port, 0}]},
    {ok, _} = ranch:start_listener(Name, Transport, TransOpts,
                                   bench_echo_handler, [{message_queue_data, MQD}]),
    Port = ranch:get_port(Name),
    timer:sleep(100),
    {ConnTime, _} = timer:tc(fun() -> bench_connect(Port, ?CLIENTS) end),
    {ThroughputTime, Latencies} = timer:tc(fun() ->
        bench_echo(Port, Payload, ?CLIENTS, ?MESSAGES_PER_CLIENT)
    end),
    ranch:stop_listener(Name),
    TotalMessages = ?CLIENTS * ?MESSAGES_PER_CLIENT,
    TotalBytes = TotalMessages * byte_size(Payload) * 2,
    #{
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
    case gen_tcp:recv(Sock, 0, 5000) of
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

print_results(Transport, R) ->
    io:format("=== ~s ===~n", [Transport]),
    io:format("  Connect ~B clients:  ~.1f ms~n", [?CLIENTS, maps:get(connect_time_us, R) / 1000]),
    io:format("  Throughput:          ~w msg/s~n", [trunc(maps:get(msg_per_sec, R))]),
    io:format("  Bandwidth:           ~.1f MB/s~n", [maps:get(mb_per_sec, R)]),
    io:format("  Latency p50:         ~w us~n", [maps:get(p50, R)]),
    io:format("  Latency p99:         ~w us~n", [maps:get(p99, R)]),
    io:format("  Latency p99.9:       ~w us~n~n", [maps:get(p999, R)]).

print_comparison(Transport, Tcp, Other) ->
    TcpMps = maps:get(msg_per_sec, Tcp),
    OtherMps = maps:get(msg_per_sec, Other),
    Diff = (OtherMps - TcpMps) / TcpMps * 100,
    Sign = case Diff >= 0 of true -> "+"; false -> "" end,
    io:format("=== Comparison ===~n"),
    io:format("  ~s vs ranch_tcp: ~s~.1f% throughput~n", [Transport, Sign, Diff]),
    io:format("  p50 latency: ~w us vs ~w us~n",
              [maps:get(p50, Other), maps:get(p50, Tcp)]),
    io:format("  p99 latency: ~w us vs ~w us~n",
              [maps:get(p99, Other), maps:get(p99, Tcp)]).
