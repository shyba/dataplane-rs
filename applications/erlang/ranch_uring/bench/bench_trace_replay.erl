-module(bench_trace_replay).

-export([run/0]).
-export([run/4]).

run() ->
    run("/tmp/ranch_tcp_uring_bench.trace", 50, 1000, 512).

run(Path, Clients, MessagesPerClient, MsgSize) ->
    application:ensure_all_started(ranch),
    application:ensure_all_started(ranch_uring),
    {module, ranch_tcp_uring} = code:ensure_loaded(ranch_tcp_uring),
    {module, ranch_tcp_uring_trace} = code:ensure_loaded(ranch_tcp_uring_trace),
    ok = ranch_tcp_uring_trace:start(Path),
    Payload = crypto:strong_rand_bytes(MsgSize),
    Name = bench_trace_ranch_tcp_uring,
    {ok, _} = ranch:start_listener(Name, ranch_tcp_uring, #{socket_opts => [{port, 0}]},
        bench_echo_handler, []),
    Port = ranch:get_port(Name),
    timer:sleep(100),
    {TimeUs, _Latencies} = timer:tc(fun() ->
        bench_echo(Port, Payload, Clients, MessagesPerClient)
    end),
    ranch:stop_listener(Name),
    timer:sleep(200),
    ok = ranch_tcp_uring_trace:stop(),
    io:format("trace_path=~s~n", [Path]),
    io:format("throughput=~w msg/s~n", [trunc((Clients * MessagesPerClient) * 1000000 / TimeUs)]),
    ok.

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
