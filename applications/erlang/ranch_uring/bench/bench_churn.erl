-module(bench_churn).

-export([run/0]).

-define(WORKERS, 50).
-define(REQUESTS_PER_WORKER, 500).
-define(MSG_SIZE, 512).
-define(LISTEN_BACKLOG, 1024).
-define(DEFAULT_RECV_TIMEOUT_MS, 1000).
-define(DEFAULT_COLLECT_TIMEOUT_MS, 10000).
-define(DEFAULT_ACCEPT_SUBS, 0).
-define(DEFAULT_ACCEPT_SUBS_AUTO, 4).
-define(STEP_RECV_ASYNC, 1).
-define(STEP_SEND_ASYNC, 2).
-define(STEP_CLOSE_ENQUEUE, 3).
-define(STEP_PENDING_WAIT, 4).
-define(STEP_DEMUX_DISPATCH, 5).
-define(STEP_COUNT, 5).

run() ->
    application:ensure_all_started(ranch),
    application:ensure_all_started(ranch_uring),
    Phase = benchmark_phase(),
    Workers = env_int("BENCH_CHURN_WORKERS", ?WORKERS),
    RequestsPerWorker = env_int("BENCH_CHURN_REQUESTS_PER_WORKER", ?REQUESTS_PER_WORKER),
    MsgSize = env_int("BENCH_CHURN_MSG_SIZE", ?MSG_SIZE),
    Payload = crypto:strong_rand_bytes(MsgSize),
    io:format("~nConnection-churn benchmark (~p): ~B workers x ~B requests x ~B bytes~n~n",
              [Phase, Workers, RequestsPerWorker, MsgSize]),
    OnlyUring = env_bool("BENCH_CHURN_ONLY_URING", false),
    RecvTimeoutMs = env_int("BENCH_CHURN_RECV_TIMEOUT_MS", ?DEFAULT_RECV_TIMEOUT_MS),
    CollectTimeoutMs = env_int("BENCH_CHURN_COLLECT_TIMEOUT_MS", ?DEFAULT_COLLECT_TIMEOUT_MS),
    UringResult = run_transport(ranch_uring, Phase, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs),
    case OnlyUring of
        true ->
            print_results(ranch_uring, UringResult);
        false ->
            TcpResult = run_transport(
                ranch_tcp,
                Phase,
                Payload,
                Workers,
                RequestsPerWorker,
                RecvTimeoutMs,
                CollectTimeoutMs
            ),
            print_results(ranch_tcp, TcpResult),
            print_results(ranch_uring, UringResult),
            print_comparison(TcpResult, UringResult)
    end,
    ok.

benchmark_phase() ->
    case os:getenv("BENCH_CHURN_PHASE") of
        "connect_close" -> connect_close;
        "recv_close" -> recv_close;
        _ -> echo
    end.

run_transport(Transport, Phase, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs) ->
    run_transport_impl(Transport, Phase, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs).

run_transport_impl(ranch_uring, Phase, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs) ->
    {ok, Port, Server} = start_subscribe_accept_server(Workers, server_mode(Phase)),
    timer:sleep(100),
    {ElapsedUs, {Latencies, StageMaxUs, RetryStats}} = timer:tc(fun() ->
        run_workers(Phase, Port, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs)
    end),
    ServerStats = get_subscribe_accept_server_stats(Server),
    print_subscribe_server_stats(ServerStats),
    stop_subscribe_accept_server(Server),
    TotalRequests = Workers * RequestsPerWorker,
    TotalBytes = TotalRequests * byte_size(Payload) * 2,
    #{
        elapsed_us => ElapsedUs,
        total_requests => TotalRequests,
        total_bytes => TotalBytes,
        req_per_sec => TotalRequests * 1000000 / ElapsedUs,
        mb_per_sec => TotalBytes / ElapsedUs,
        latencies_us => Latencies,
        stage_max_us => StageMaxUs,
        connect_retry_stats => RetryStats,
        max => lists:last(Latencies),
        p50 => percentile(Latencies, 50),
        p99 => percentile(Latencies, 99),
        p999 => percentile(Latencies, 99.9),
        p9999 => percentile(Latencies, 99.99)
    };
run_transport_impl(Transport, Phase, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs) ->
    Name = list_to_atom(
        "bench_churn_" ++ atom_to_list(Transport) ++ "_" ++
        integer_to_list(erlang:unique_integer([positive]))
    ),
    TransOpts = #{socket_opts => [{port, 0}]},
    catch ranch:stop_listener(Name),
    {ok, _} = ranch:start_listener(Name, Transport, TransOpts, phase_handler(Phase), []),
    Port = ranch:get_port(Name),
    timer:sleep(100),
    {ElapsedUs, {Latencies, StageMaxUs, RetryStats}} = timer:tc(fun() ->
        run_workers(Phase, Port, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs)
    end),
    ranch:stop_listener(Name),
    TotalRequests = Workers * RequestsPerWorker,
    TotalBytes = TotalRequests * byte_size(Payload) * 2,
    #{
        elapsed_us => ElapsedUs,
        total_requests => TotalRequests,
        total_bytes => TotalBytes,
        req_per_sec => TotalRequests * 1000000 / ElapsedUs,
        mb_per_sec => TotalBytes / ElapsedUs,
        latencies_us => Latencies,
        stage_max_us => StageMaxUs,
        connect_retry_stats => RetryStats,
        max => lists:last(Latencies),
        p50 => percentile(Latencies, 50),
        p99 => percentile(Latencies, 99),
        p999 => percentile(Latencies, 99.9),
        p9999 => percentile(Latencies, 99.99)
    }.

server_mode(Phase) ->
    case os:getenv("BENCH_CHURN_SERVER_MODE") of
        "synthetic_async" -> synthetic_async;
        _ ->
            case Phase of
                connect_close -> accept_close;
                recv_close -> recv_close;
                echo -> normal
            end
    end.

phase_handler(connect_close) ->
    bench_close_handler;
phase_handler(recv_close) ->
    bench_recv_close_handler;
phase_handler(echo) ->
    bench_echo_handler.

start_subscribe_accept_server(_Workers, Mode) ->
    Parent = self(),
    SubsOverride = env_int("BENCH_CHURN_ACCEPT_SUBS", ?DEFAULT_ACCEPT_SUBS),
    DemuxWorkers = env_int("BENCH_CHURN_DEMUX_WORKERS", 1),
    % Accept subscriptions for one listener all route by listener FD, so scaling
    % with worker count just overloads a single shard/collector.
    SubsAuto = ?DEFAULT_ACCEPT_SUBS_AUTO,
    Subs = case SubsOverride of
        N when N > 0 -> max(1, min(N, 64));
        _ -> SubsAuto
    end,
    {Pid, Ref} = spawn_monitor(fun() -> subscribe_accept_server(Parent, Subs, DemuxWorkers, Mode) end),
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

get_subscribe_accept_server_stats({Pid, _Ref}) ->
    Pid ! {get_stats, self()},
    receive
        {subscribe_accept_stats, Stats} ->
            Stats
    after 1000 ->
        timeout
    end.

subscribe_accept_server(Parent, SubscriptionCount, DemuxWorkers0, Mode) ->
    DemuxWorkers = max(1, DemuxWorkers0),
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
                    case start_accept_demux_workers(Fd, SubscriptionCount, DemuxWorkers, Mode) of
                        {ok, DemuxWorkersState} ->
                            {ok, {_, Port}} = inet:sockname(ListenSock),
                            Parent ! {subscribe_accept_ready, self(), Port},
                            subscribe_accept_server_loop(ListenSock, DemuxWorkersState);
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

start_accept_demux_workers(_Fd, _SubscriptionCount, 0, _Mode) ->
    {error, no_demux_workers};
start_accept_demux_workers(Fd, SubscriptionCount, N, Mode) ->
    start_accept_demux_workers(Fd, SubscriptionCount, N, Mode, []).

start_accept_demux_workers(_Fd, _SubscriptionCount, 0, _Mode, Acc) ->
    {ok, lists:reverse(Acc)};
start_accept_demux_workers(Fd, SubscriptionCount, N, Mode, Acc) ->
    Parent = self(),
    {Pid, Ref} = spawn_monitor(fun() ->
        subscribe_accept_demux_worker(Parent, Fd, SubscriptionCount, Mode)
    end),
    receive
        {subscribe_demux_ready, Pid} ->
            start_accept_demux_workers(Fd, SubscriptionCount, N - 1, Mode, [{Pid, Ref} | Acc]);
        {subscribe_demux_error, Pid, Reason} ->
            demonitor(Ref, [flush]),
            stop_demux_workers(Acc),
            {error, Reason};
        {'DOWN', Ref, process, Pid, Reason} ->
            stop_demux_workers(Acc),
            {error, Reason}
    after 5000 ->
        exit(Pid, kill),
        stop_demux_workers(Acc),
        {error, demux_start_timeout}
    end.

subscribe_accept_server_loop(ListenSock, DemuxWorkers) ->
    receive
        {get_stats, From} ->
            From ! {subscribe_accept_stats, collect_demux_stats(DemuxWorkers)},
            subscribe_accept_server_loop(ListenSock, DemuxWorkers);
        stop ->
            stop_demux_workers(DemuxWorkers),
            gen_tcp:close(ListenSock),
            ok;
        {'DOWN', Ref, process, Pid, Reason} ->
            Remaining = lists:keydelete(Pid, 1, DemuxWorkers),
            case lists:keyfind(Pid, 1, DemuxWorkers) of
                false ->
                    subscribe_accept_server_loop(ListenSock, Remaining);
                _ ->
                    stop_demux_workers(Remaining),
                    gen_tcp:close(ListenSock),
                    exit({demux_down, Pid, Ref, Reason})
            end
    end.

subscribe_accept_demux_worker(Parent, Fd, SubscriptionCount, Mode) ->
    case start_accept_subscriptions(Fd, SubscriptionCount) of
        {ok, SubscriptionIds} ->
            Parent ! {subscribe_demux_ready, self()},
            SubscriptionSet = maps:from_keys(SubscriptionIds, true),
            subscribe_accept_loop(
                SubscriptionIds,
                SubscriptionSet,
                undefined,
                Mode,
                #{},
                empty_subscribe_server_stats()
            );
        Error ->
            Parent ! {subscribe_demux_error, self(), Error}
    end.

stop_demux_workers(DemuxWorkers) ->
    lists:foreach(fun({Pid, _Ref}) -> Pid ! stop end, DemuxWorkers),
    lists:foreach(
        fun({Pid, Ref}) ->
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
            end
        end,
        DemuxWorkers
    ),
    ok.

collect_demux_stats([]) ->
    empty_subscribe_server_stats();
collect_demux_stats(DemuxWorkers) ->
    RequestRef = make_ref(),
    lists:foreach(fun({Pid, _Ref}) -> Pid ! {get_stats, self(), RequestRef} end, DemuxWorkers),
    gather_demux_stats(length(DemuxWorkers), RequestRef, empty_subscribe_server_stats()).

gather_demux_stats(0, _RequestRef, Acc) ->
    Acc;
gather_demux_stats(Remaining, RequestRef, Acc) ->
    receive
        {subscribe_demux_stats, RequestRef, Stats} ->
            gather_demux_stats(Remaining - 1, RequestRef, merge_subscribe_server_stats(Acc, Stats))
    after 1000 ->
        Acc
    end.

start_accept_subscriptions(Fd, N) ->
    start_accept_subscriptions(Fd, max(1, N), []).

start_accept_subscriptions(_Fd, 0, Acc) ->
    {ok, lists:reverse(Acc)};
start_accept_subscriptions(Fd, N, Acc) ->
    case ranch_uring_nif:command({subscribe, self(), accept, Fd}) of
        {ok, SubscriptionId} ->
            start_accept_subscriptions(Fd, N - 1, [SubscriptionId | Acc]);
        Error ->
            lists:foreach(fun(Id) -> _ = ranch_uring_nif:command({stop, Id}) end, Acc),
            Error
    end.

subscribe_accept_loop(SubscriptionIds, SubscriptionSet, ListenSock, Mode, Pending, Stats) ->
    StatsIn = stats_observe_mailbox_peak(Stats),
    receive
        {get_stats, From} ->
            From ! {subscribe_accept_stats, StatsIn},
            subscribe_accept_loop(SubscriptionIds, SubscriptionSet, ListenSock, Mode, Pending, StatsIn);
        {get_stats, From, RequestRef} ->
            From ! {subscribe_demux_stats, RequestRef, StatsIn},
            subscribe_accept_loop(SubscriptionIds, SubscriptionSet, ListenSock, Mode, Pending, StatsIn);
        stop ->
            close_pending_sessions(Pending),
            lists:foreach(
                fun(Id) ->
                    _ = ranch_uring_nif:command({stop, Id})
                end,
                SubscriptionIds
            ),
            case ListenSock of
                undefined -> ok;
                _ -> gen_tcp:close(ListenSock)
            end,
            ok;
        {reply, RequestId, {ok, Session}} when is_integer(RequestId) ->
            T0 = now_ns(),
            {Pending1, Stats1} = handle_server_reply(
                SubscriptionSet,
                Mode,
                RequestId,
                {ok, Session},
                Pending,
                StatsIn
            ),
            T1 = now_ns(),
            Stats2 = stats_step_observe(?STEP_DEMUX_DISPATCH, T1 - T0, stats_inc(demux_single_count, 1, Stats1)),
            subscribe_accept_loop(SubscriptionIds, SubscriptionSet, ListenSock, Mode, Pending1, Stats2);
        {reply, RequestId, {error, _Reason}} when is_integer(RequestId) ->
            T0 = now_ns(),
            {Pending1, Stats1} = handle_server_reply(
                SubscriptionSet,
                Mode,
                RequestId,
                {error, failed},
                Pending,
                StatsIn
            ),
            T1 = now_ns(),
            Stats2 = stats_step_observe(?STEP_DEMUX_DISPATCH, T1 - T0, stats_inc(demux_single_count, 1, Stats1)),
            subscribe_accept_loop(SubscriptionIds, SubscriptionSet, ListenSock, Mode, Pending1, Stats2);
        {reply, RequestId, ok} when is_integer(RequestId) ->
            T0 = now_ns(),
            {Pending1, Stats1} = handle_server_reply(
                SubscriptionSet,
                Mode,
                RequestId,
                ok,
                Pending,
                StatsIn
            ),
            T1 = now_ns(),
            Stats2 = stats_step_observe(?STEP_DEMUX_DISPATCH, T1 - T0, stats_inc(demux_single_count, 1, Stats1)),
            subscribe_accept_loop(SubscriptionIds, SubscriptionSet, ListenSock, Mode, Pending1, Stats2);
        {nif_results, Pairs} ->
            T0 = now_ns(),
            Stats0 = stats_inc(demux_batch_count, 1, stats_inc(demux_batch_items, length(Pairs), StatsIn)),
            {Pending1, Stats1, _} = lists:foldl(
                fun({RequestId, Result}, {AccPending, AccStats, Idx}) ->
                    ItemNs = now_ns(),
                    SeqDelayNs = ItemNs - T0,
                    AccStats1 = stats_observe_batch_item_seq_delay(SeqDelayNs, AccStats),
                    {NextPending, NextStats} =
                        handle_server_reply(SubscriptionSet, Mode, RequestId, Result, AccPending, AccStats1),
                    {NextPending, NextStats, Idx + 1}
                end,
                {Pending, Stats0, 0},
                Pairs
            ),
            T1 = now_ns(),
            Stats2 = stats_step_observe(?STEP_DEMUX_DISPATCH, T1 - T0, Stats1),
            subscribe_accept_loop(SubscriptionIds, SubscriptionSet, ListenSock, Mode, Pending1, Stats2);
        _Other ->
            subscribe_accept_loop(SubscriptionIds, SubscriptionSet, ListenSock, Mode, Pending, StatsIn)
    end.

handle_server_reply(SubscriptionSet, Mode, RequestId, Result, Pending, Stats) ->
    case maps:is_key(RequestId, SubscriptionSet) of
        true ->
            handle_accept_result(Mode, Result, Pending, Stats);
        false ->
            handle_pending_result(RequestId, Result, Pending, Stats)
    end.

handle_accept_result(normal, {ok, Session}, Pending, Stats) ->
    spawn(fun() -> echo_subscribed_session(Session) end),
    {Pending, Stats};
handle_accept_result(normal, _Result, Pending, Stats) ->
    {Pending, Stats};
handle_accept_result(accept_close, {ok, Session}, Pending, Stats0) ->
    T0 = now_ns(),
    _ = ranch_uring_nif:close_enqueue(Session),
    T1 = now_ns(),
    {Pending, stats_step_observe(?STEP_CLOSE_ENQUEUE, T1 - T0, Stats0)};
handle_accept_result(accept_close, _Result, Pending, Stats) ->
    {Pending, Stats};
handle_accept_result(recv_close, {ok, Session}, Pending, Stats0) ->
    T0 = now_ns(),
    case ranch_uring_nif:recv_async(Session, 0) of
        {ok, RequestId} ->
            T1 = now_ns(),
            Stats1 = stats_step_observe(?STEP_RECV_ASYNC, T1 - T0, Stats0),
            Pending1 = Pending#{RequestId => {recv_close, Session, T1}},
            {Pending1, stats_observe_pending(Pending1, Stats1)};
        {error, _} ->
            T1 = now_ns(),
            Stats1 = stats_step_observe(?STEP_RECV_ASYNC, T1 - T0, Stats0),
            T2 = now_ns(),
            _ = ranch_uring_nif:close_enqueue(Session),
            T3 = now_ns(),
            {Pending, stats_step_observe(?STEP_CLOSE_ENQUEUE, T3 - T2, Stats1)}
    end;
handle_accept_result(recv_close, _Result, Pending, Stats) ->
    {Pending, Stats};
handle_accept_result(synthetic_async, {ok, Session}, Pending, Stats0) ->
    T0 = now_ns(),
    case ranch_uring_nif:recv_async(Session, 0) of
        {ok, RequestId} ->
            T1 = now_ns(),
            Stats1 = stats_step_observe(?STEP_RECV_ASYNC, T1 - T0, Stats0),
            Pending1 = Pending#{RequestId => {recv, Session, T1}},
            {Pending1, stats_observe_pending(Pending1, Stats1)};
        {error, _} ->
            T1 = now_ns(),
            Stats1 = stats_step_observe(?STEP_RECV_ASYNC, T1 - T0, Stats0),
            T2 = now_ns(),
            _ = ranch_uring_nif:close_enqueue(Session),
            T3 = now_ns(),
            {Pending, stats_step_observe(?STEP_CLOSE_ENQUEUE, T3 - T2, Stats1)}
    end;
handle_accept_result(synthetic_async, _Result, Pending, Stats) ->
    {Pending, Stats}.

handle_pending_result(RequestId, Result, Pending0, Stats0) ->
    case maps:take(RequestId, Pending0) of
        {{recv, Session, QueuedNs}, Pending1} ->
            TWait = now_ns(),
            Stats1 = stats_step_observe(?STEP_PENDING_WAIT, TWait - QueuedNs, Stats0),
            case Result of
                {ok, Data} when is_binary(Data) ->
                    T0 = now_ns(),
                    case ranch_uring_nif:send_async(Session, Data) of
                        {ok, SendRequestId} ->
                            T1 = now_ns(),
                            Stats2 = stats_step_observe(?STEP_SEND_ASYNC, T1 - T0, Stats1),
                            Pending2 = Pending1#{SendRequestId => {send, Session, T1}},
                            {Pending2, stats_observe_pending(Pending2, Stats2)};
                        {error, _} ->
                            T1 = now_ns(),
                            Stats2 = stats_step_observe(?STEP_SEND_ASYNC, T1 - T0, Stats1),
                            T2 = now_ns(),
                            _ = ranch_uring_nif:close_enqueue(Session),
                            T3 = now_ns(),
                            {Pending1, stats_step_observe(?STEP_CLOSE_ENQUEUE, T3 - T2, Stats2)}
                    end;
                _ ->
                    T2 = now_ns(),
                    _ = ranch_uring_nif:close_enqueue(Session),
                    T3 = now_ns(),
                    {Pending1, stats_step_observe(?STEP_CLOSE_ENQUEUE, T3 - T2, Stats1)}
            end;
        {{recv_close, Session, QueuedNs}, Pending1} ->
            TWait = now_ns(),
            Stats1 = stats_step_observe(?STEP_PENDING_WAIT, TWait - QueuedNs, Stats0),
            T0 = now_ns(),
            _ = ranch_uring_nif:close_enqueue(Session),
            T1 = now_ns(),
            {Pending1, stats_step_observe(?STEP_CLOSE_ENQUEUE, T1 - T0, Stats1)};
        {{send, Session, QueuedNs}, Pending1} ->
            TWait = now_ns(),
            Stats1 = stats_step_observe(?STEP_PENDING_WAIT, TWait - QueuedNs, Stats0),
            T0 = now_ns(),
            _ = ranch_uring_nif:close_enqueue(Session),
            T1 = now_ns(),
            {Pending1, stats_step_observe(?STEP_CLOSE_ENQUEUE, T1 - T0, Stats1)};
        error ->
            {Pending0, stats_inc(pending_miss_count, 1, Stats0)}
    end.

close_pending_sessions(Pending) ->
    maps:foreach(
        fun(_RequestId, PendingItem) ->
            case PendingItem of
                {_Stage, Session, _QueuedNs} ->
                    _ = ranch_uring_nif:close_enqueue(Session);
                {_Stage, Session} ->
                    _ = ranch_uring_nif:close_enqueue(Session)
            end
        end,
        Pending
    ).

empty_subscribe_server_stats() ->
    Z = erlang:make_tuple(?STEP_COUNT, 0),
    #{
        step_counts => Z,
        step_totals_ns => Z,
        step_max_ns => Z,
        demux_single_count => 0,
        demux_batch_count => 0,
        demux_batch_items => 0,
        pending_peak => 0,
        pending_miss_count => 0,
        mailbox_queue_len_peak => 0,
        batch_item_seq_count => 0,
        batch_item_seq_total_ns => 0,
        batch_item_seq_max_ns => 0
    }.

stats_inc(Key, Delta, Stats) ->
    Stats#{Key => maps:get(Key, Stats, 0) + Delta}.

stats_step_observe(Step, Ns0, Stats) ->
    Ns = max(0, Ns0),
    Counts0 = maps:get(step_counts, Stats),
    Totals0 = maps:get(step_totals_ns, Stats),
    Max0 = maps:get(step_max_ns, Stats),
    C = element(Step, Counts0),
    T = element(Step, Totals0),
    M = element(Step, Max0),
    Stats#{
        step_counts => setelement(Step, Counts0, C + 1),
        step_totals_ns => setelement(Step, Totals0, T + Ns),
        step_max_ns => setelement(Step, Max0, max(M, Ns))
    }.

stats_observe_pending(Pending, Stats) ->
    Size = map_size(Pending),
    Peak0 = maps:get(pending_peak, Stats, 0),
    Stats#{pending_peak => max(Peak0, Size)}.

stats_observe_mailbox_peak(Stats) ->
    case process_info(self(), message_queue_len) of
        {message_queue_len, Len} when is_integer(Len), Len >= 0 ->
            Peak0 = maps:get(mailbox_queue_len_peak, Stats, 0),
            Stats#{mailbox_queue_len_peak => max(Peak0, Len)};
        _ ->
            Stats
    end.

stats_observe_batch_item_seq_delay(Ns0, Stats) ->
    Ns = max(0, Ns0),
    Count0 = maps:get(batch_item_seq_count, Stats, 0),
    Total0 = maps:get(batch_item_seq_total_ns, Stats, 0),
    Max0 = maps:get(batch_item_seq_max_ns, Stats, 0),
    Stats#{
        batch_item_seq_count => Count0 + 1,
        batch_item_seq_total_ns => Total0 + Ns,
        batch_item_seq_max_ns => max(Max0, Ns)
    }.

merge_subscribe_server_stats(A, B) ->
    #{
        step_counts => tuple_add(maps:get(step_counts, A), maps:get(step_counts, B)),
        step_totals_ns => tuple_add(maps:get(step_totals_ns, A), maps:get(step_totals_ns, B)),
        step_max_ns => tuple_max(maps:get(step_max_ns, A), maps:get(step_max_ns, B)),
        demux_single_count => maps:get(demux_single_count, A) + maps:get(demux_single_count, B),
        demux_batch_count => maps:get(demux_batch_count, A) + maps:get(demux_batch_count, B),
        demux_batch_items => maps:get(demux_batch_items, A) + maps:get(demux_batch_items, B),
        pending_peak => max(maps:get(pending_peak, A), maps:get(pending_peak, B)),
        pending_miss_count => maps:get(pending_miss_count, A) + maps:get(pending_miss_count, B),
        mailbox_queue_len_peak => max(
            maps:get(mailbox_queue_len_peak, A),
            maps:get(mailbox_queue_len_peak, B)
        ),
        batch_item_seq_count => maps:get(batch_item_seq_count, A) + maps:get(batch_item_seq_count, B),
        batch_item_seq_total_ns => maps:get(batch_item_seq_total_ns, A) + maps:get(batch_item_seq_total_ns, B),
        batch_item_seq_max_ns => max(maps:get(batch_item_seq_max_ns, A), maps:get(batch_item_seq_max_ns, B))
    }.

tuple_add(A, B) ->
    tuple_add(1, tuple_size(A), A, B, erlang:make_tuple(tuple_size(A), 0)).

tuple_add(I, N, _A, _B, Out) when I > N ->
    Out;
tuple_add(I, N, A, B, Out0) ->
    Out1 = setelement(I, Out0, element(I, A) + element(I, B)),
    tuple_add(I + 1, N, A, B, Out1).

tuple_max(A, B) ->
    tuple_max(1, tuple_size(A), A, B, erlang:make_tuple(tuple_size(A), 0)).

tuple_max(I, N, _A, _B, Out) when I > N ->
    Out;
tuple_max(I, N, A, B, Out0) ->
    Out1 = setelement(I, Out0, max(element(I, A), element(I, B))),
    tuple_max(I + 1, N, A, B, Out1).

now_ns() ->
    erlang:monotonic_time(nanosecond).

print_subscribe_server_stats(timeout) ->
    io:format("  subscribe demux stats: timeout~n");
print_subscribe_server_stats(Stats) ->
    io:format("  subscribe demux calls (Erlang side):~n"),
    print_step_timing("recv_async", ?STEP_RECV_ASYNC, Stats),
    print_step_timing("send_async", ?STEP_SEND_ASYNC, Stats),
    print_step_timing("close_enqueue", ?STEP_CLOSE_ENQUEUE, Stats),
    print_step_timing("pending_wait", ?STEP_PENDING_WAIT, Stats),
    print_step_timing("demux_dispatch", ?STEP_DEMUX_DISPATCH, Stats),
    io:format("    demux_single_count: ~B~n", [maps:get(demux_single_count, Stats)]),
    io:format("    demux_batch_count:  ~B~n", [maps:get(demux_batch_count, Stats)]),
    io:format("    demux_batch_items:  ~B~n", [maps:get(demux_batch_items, Stats)]),
    io:format("    pending_peak:       ~B~n", [maps:get(pending_peak, Stats)]),
    io:format("    pending_miss_count: ~B~n", [maps:get(pending_miss_count, Stats)]),
    io:format("    mailbox_q_peak:     ~B~n", [maps:get(mailbox_queue_len_peak, Stats)]),
    BatchSeqCount = maps:get(batch_item_seq_count, Stats, 0),
    BatchSeqTotal = maps:get(batch_item_seq_total_ns, Stats, 0),
    BatchSeqMax = maps:get(batch_item_seq_max_ns, Stats, 0),
    BatchSeqAvgUs = case BatchSeqCount of
        0 -> 0.0;
        _ -> BatchSeqTotal / BatchSeqCount / 1000
    end,
    io:format("    batch_item_seq avg=~.2f us max=~.2f us~n~n", [BatchSeqAvgUs, BatchSeqMax / 1000]).

print_step_timing(Label, Step, Stats) ->
    Counts = maps:get(step_counts, Stats),
    Totals = maps:get(step_totals_ns, Stats),
    Maxes = maps:get(step_max_ns, Stats),
    Count = element(Step, Counts),
    TotalNs = element(Step, Totals),
    MaxNs = element(Step, Maxes),
    AvgUs = case Count of
        0 -> 0.0;
        _ -> TotalNs / Count / 1000
    end,
    io:format("    ~s count=~B avg=~.2f us max=~.2f us~n",
              [Label, Count, AvgUs, MaxNs / 1000]).

echo_subscribed_session(Session) ->
    case ranch_uring_nif:recv(Session, 0, 5000) of
        {ok, Data} when is_binary(Data) ->
            _ = ranch_uring_nif:send(Session, Data),
            ok;
        _ ->
            _ = ranch_uring_nif:close(Session),
            ok
    end,
    ok.

run_workers(Phase, Port, Payload, Workers, RequestsPerWorker, RecvTimeoutMs, CollectTimeoutMs) ->
    Self = self(),
    Pids = [spawn_link(fun() ->
        {Lats, StageMax, RetryStats} = churn_loop(
            Phase,
            Port,
            Payload,
            RequestsPerWorker,
            RecvTimeoutMs,
            [],
            empty_stage_max_us(),
            empty_retry_stats()
        ),
        Self ! {done, self(), Lats, StageMax, RetryStats}
    end) || _ <- lists:seq(1, Workers)],
    collect_latencies(Pids, [], empty_stage_max_us(), empty_retry_stats(), CollectTimeoutMs).

churn_loop(_Phase, _Port, _Payload, 0, _RecvTimeoutMs, Acc, StageMaxUs, RetryStats) ->
    {Acc, StageMaxUs, RetryStats};
churn_loop(Phase, Port, Payload, Remaining, RecvTimeoutMs, Acc, StageMaxUs0, RetryStats0) ->
    Size = byte_size(Payload),
    T0 = erlang:monotonic_time(microsecond),
    TConnect0 = T0,
    {ok, Sock, ConnectRetries} = connect_with_retry(Port, 100),
    TConnect1 = erlang:monotonic_time(microsecond),
    {ok, {SendUs, RecvUs, CloseUs}} = run_client_phase(Phase, Sock, Payload, Size, RecvTimeoutMs),
    T1 = erlang:monotonic_time(microsecond),
    StageMaxUs1 = stage_max_observe_retry(
        {TConnect1 - TConnect0, SendUs, RecvUs, CloseUs},
        ConnectRetries,
        StageMaxUs0
    ),
    RetryStats1 = retry_stats_observe(ConnectRetries, RetryStats0),
    churn_loop(
        Phase,
        Port,
        Payload,
        Remaining - 1,
        RecvTimeoutMs,
        [T1 - T0 | Acc],
        StageMaxUs1,
        RetryStats1
    ).

run_client_phase(connect_close, Sock, _Payload, _Size, _RecvTimeoutMs) ->
    TClose0 = erlang:monotonic_time(microsecond),
    ok = gen_tcp:close(Sock),
    TClose1 = erlang:monotonic_time(microsecond),
    {ok, {0, 0, TClose1 - TClose0}};
run_client_phase(recv_close, Sock, Payload, _Size, RecvTimeoutMs) ->
    TSend0 = erlang:monotonic_time(microsecond),
    ok = gen_tcp:send(Sock, Payload),
    ok = gen_tcp:shutdown(Sock, write),
    TSend1 = erlang:monotonic_time(microsecond),
    TRecv0 = erlang:monotonic_time(microsecond),
    case gen_tcp:recv(Sock, 0, RecvTimeoutMs) of
        {error, closed} ->
            TRecv1 = erlang:monotonic_time(microsecond),
            {ok, {TSend1 - TSend0, TRecv1 - TRecv0, 0}};
        {ok, _Data} ->
            TRecv1 = erlang:monotonic_time(microsecond),
            TClose0 = erlang:monotonic_time(microsecond),
            ok = gen_tcp:close(Sock),
            TClose1 = erlang:monotonic_time(microsecond),
            {ok, {TSend1 - TSend0, TRecv1 - TRecv0, TClose1 - TClose0}};
        {error, _} = Err ->
            Err
    end;
run_client_phase(echo, Sock, Payload, Size, RecvTimeoutMs) ->
    TSend0 = erlang:monotonic_time(microsecond),
    ok = gen_tcp:send(Sock, Payload),
    TSend1 = erlang:monotonic_time(microsecond),
    TRecv0 = erlang:monotonic_time(microsecond),
    {ok, _} = recv_exact(Sock, Size, <<>>, RecvTimeoutMs),
    TRecv1 = erlang:monotonic_time(microsecond),
    TClose0 = erlang:monotonic_time(microsecond),
    ok = gen_tcp:close(Sock),
    TClose1 = erlang:monotonic_time(microsecond),
    {ok, {TSend1 - TSend0, TRecv1 - TRecv0, TClose1 - TClose0}}.

connect_with_retry(Port, AttemptsLeft) ->
    connect_with_retry(Port, AttemptsLeft, 0).

connect_with_retry(Port, AttemptsLeft, Retries) ->
    case gen_tcp:connect({127,0,0,1}, Port, [binary, {active, false}, {packet, raw}]) of
        {ok, Sock} ->
            {ok, Sock, Retries};
        {error, eaddrnotavail} when AttemptsLeft > 0 ->
            timer:sleep(5),
            connect_with_retry(Port, AttemptsLeft - 1, Retries + 1);
        {error, eaddrinuse} when AttemptsLeft > 0 ->
            timer:sleep(5),
            connect_with_retry(Port, AttemptsLeft - 1, Retries + 1);
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

collect_latencies([], Acc, StageMaxUs, RetryStats, _CollectTimeoutMs) ->
    {lists:sort(lists:flatten(Acc)), StageMaxUs, RetryStats};
collect_latencies(Pids, Acc, StageMaxUs0, RetryStats0, CollectTimeoutMs) ->
    receive
        {done, Pid, Lats, StageMaxUs1, RetryStats1} ->
            collect_latencies(
                lists:delete(Pid, Pids),
                [Lats | Acc],
                stage_max_merge(StageMaxUs0, StageMaxUs1),
                retry_stats_merge(RetryStats0, RetryStats1),
                CollectTimeoutMs
            )
    after CollectTimeoutMs ->
        error(churn_timeout)
    end.

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

env_bool(Name, Default) ->
    case os:getenv(Name) of
        false -> Default;
        [] -> Default;
        "1" -> true;
        "true" -> true;
        "TRUE" -> true;
        "yes" -> true;
        "YES" -> true;
        "on" -> true;
        "ON" -> true;
        _ -> false
    end.

empty_stage_max_us() ->
    #{
        connect => 0,
        send => 0,
        recv => 0,
        close => 0,
        connect_retries => 0
    }.

stage_max_observe({ConnectUs, SendUs, RecvUs, CloseUs}, StageMaxUs0) ->
    StageMaxUs0#{
        connect => max(maps:get(connect, StageMaxUs0), ConnectUs),
        send => max(maps:get(send, StageMaxUs0), SendUs),
        recv => max(maps:get(recv, StageMaxUs0), RecvUs),
        close => max(maps:get(close, StageMaxUs0), CloseUs)
    }.

stage_max_observe_retry(StageUs, ConnectRetries, StageMaxUs0) ->
    StageMaxUs1 = stage_max_observe(StageUs, StageMaxUs0),
    StageMaxUs1#{
        connect_retries => max(maps:get(connect_retries, StageMaxUs0), ConnectRetries)
    }.

stage_max_merge(A, B) ->
    #{
        connect => max(maps:get(connect, A), maps:get(connect, B)),
        send => max(maps:get(send, A), maps:get(send, B)),
        recv => max(maps:get(recv, A), maps:get(recv, B)),
        close => max(maps:get(close, A), maps:get(close, B)),
        connect_retries => max(maps:get(connect_retries, A), maps:get(connect_retries, B))
    }.

empty_retry_stats() ->
    #{
        retries_total => 0,
        retries_max => 0
    }.

retry_stats_merge(A, B) ->
    #{
        retries_total => maps:get(retries_total, A) + maps:get(retries_total, B),
        retries_max => max(maps:get(retries_max, A), maps:get(retries_max, B))
    }.

retry_stats_observe(Retries, Stats0) ->
    #{
        retries_total => maps:get(retries_total, Stats0) + Retries,
        retries_max => max(maps:get(retries_max, Stats0), Retries)
    }.

percentile(Sorted, P) ->
    Len = length(Sorted),
    Idx = max(1, min(Len, round(P / 100 * Len))),
    lists:nth(Idx, Sorted).

print_results(Transport, R) ->
    io:format("=== ~s (churn) ===~n", [Transport]),
    io:format("  Requests:            ~B~n", [maps:get(total_requests, R)]),
    io:format("  Throughput:          ~w req/s~n", [trunc(maps:get(req_per_sec, R))]),
    io:format("  Bandwidth:           ~.1f MB/s~n", [maps:get(mb_per_sec, R)]),
    io:format("  Latency p50:         ~w us~n", [maps:get(p50, R)]),
    io:format("  Latency p99:         ~w us~n", [maps:get(p99, R)]),
    io:format("  Latency p99.9:       ~w us~n", [maps:get(p999, R)]),
    io:format("  Latency p99.99:      ~w us~n", [maps:get(p9999, R)]),
    io:format("  Latency max:         ~w us~n", [maps:get(max, R)]),
    StageMaxUs = maps:get(stage_max_us, R),
    io:format("  Stage max (us):      connect=~w send=~w recv=~w close=~w retries_max=~w~n",
              [
                maps:get(connect, StageMaxUs),
                maps:get(send, StageMaxUs),
                maps:get(recv, StageMaxUs),
                maps:get(close, StageMaxUs),
                maps:get(connect_retries, StageMaxUs, 0)
              ]),
    RetryStats = maps:get(connect_retry_stats, R, empty_retry_stats()),
    io:format("  Connect retries:     total=~w max=~w~n~n",
              [
                maps:get(retries_total, RetryStats, 0),
                maps:get(retries_max, RetryStats, 0)
              ]).

print_comparison(Tcp, Uring) ->
    TcpRps = maps:get(req_per_sec, Tcp),
    UringRps = maps:get(req_per_sec, Uring),
    Diff = (UringRps - TcpRps) / TcpRps * 100,
    Sign = case Diff >= 0 of true -> "+"; false -> "" end,
    io:format("=== Comparison ===~n"),
    io:format("  ranch_uring vs ranch_tcp: ~s~.1f% throughput~n", [Sign, Diff]),
    io:format("  p50 latency: ~w us vs ~w us~n",
              [maps:get(p50, Uring), maps:get(p50, Tcp)]),
    io:format("  p99 latency: ~w us vs ~w us~n",
              [maps:get(p99, Uring), maps:get(p99, Tcp)]).
