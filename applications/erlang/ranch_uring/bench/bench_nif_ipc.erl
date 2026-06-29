-module(bench_nif_ipc).

-export([run/0, queue_write_bench/3, queue_read_bench/3, zero_read_bench/2, zero_read_ipc_bench/2, zero_read_owned_ipc_bench/2, zero_read_env_roundtrip_bench/2, zero_read_owned_roundtrip_bench/2, result_batch_bench/3, runtime_nop_bench/1]).

-define(OP_READ, read).
-define(OP_WRITE, write).
-define(REPLY_TAG_OK, bench_reply).
-define(REPLY_TAG_BIN, bench_reply_bin).
-define(REPLY_TAG_BATCH, bench_reply_batch).
-define(REPLY_TAG_QUEUE, bench_queue_reply).
-define(N_OK, 1000000).
-define(N_BIN, 100000).
-define(BIN_SIZE, 64).
-define(BATCH_OPS, 200000).
-define(BATCH_SIZES, [1, 8, 32, 128]).
-define(QUEUE_PAYLOAD_SIZES, [0, 64, 512, 4096, 16384, 65536]).
-define(QUEUE_BATCH_SIZES, [1, 8, 32]).
-define(ZERO_READ_ITERS, 500000).
-define(ZERO_READ_SIZES, [64, 512, 4096]).
-define(RESULT_BATCH_COUNTS, [4, 16, 64]).

run() ->
    application:ensure_all_started(ranch_uring),
    flush_mailbox(),
    warmup(),
    io:format("~nRustler IPC vs NIF benchmark~n"),
    io:format("  ok iterations:    ~B~n", [?N_OK]),
    io:format("  binary iterations: ~B x ~B bytes~n~n", [?N_BIN, ?BIN_SIZE]),
    DirectOk = time(fun() -> direct_ok_loop(?N_OK, 0) end),
    EnvSendOk = time(fun() -> nif_send_ok_run(?N_OK) end),
    OwnedEnvSendOk = time(fun() -> nif_owned_send_ok_run(?N_OK) end),
    ErlSendOk = time(fun() -> erl_send_ok_run(?N_OK) end),
    DirectBin = time(fun() -> direct_binary_loop(?N_BIN, ?BIN_SIZE, 0) end),
    EnvSendBin = time(fun() -> nif_send_binary_run(?N_BIN, ?BIN_SIZE) end),
    OwnedEnvSendBin = time(fun() -> nif_owned_send_binary_run(?N_BIN, ?BIN_SIZE) end),
    ErlSendBin = time(fun() -> erl_send_binary_run(?N_BIN, ?BIN_SIZE) end),
    print_result("nif_direct_ok", DirectOk, ?N_OK),
    print_result("nif_env_send_ok", EnvSendOk, ?N_OK),
    print_result("nif_owned_env_send_ok", OwnedEnvSendOk, ?N_OK),
    print_result("erl_send_ok", ErlSendOk, ?N_OK),
    io:format("~n"),
    print_result("nif_direct_binary", DirectBin, ?N_BIN),
    print_result("nif_env_send_binary", EnvSendBin, ?N_BIN),
    print_result("nif_owned_env_send_binary", OwnedEnvSendBin, ?N_BIN),
    print_result("erl_send_binary", ErlSendBin, ?N_BIN),
    io:format("~nBatched NIF boundary (~B total commands, ~B-byte payloads)~n", [?BATCH_OPS, ?BIN_SIZE]),
    lists:foreach(fun(Size) -> print_batch(Size) end, ?BATCH_SIZES),
    io:format("~nLock-free queue ingress batch (worker-thread reply)~n", []),
    lists:foreach(fun(Size) -> print_queue_batch(Size) end, ?QUEUE_PAYLOAD_SIZES),
    io:format("~nZero read loop (Erlang loop, Rust-emulated /dev/zero)~n", []),
    lists:foreach(fun(Size) -> print_zero_read(Size) end, ?ZERO_READ_SIZES),
    io:format("~nBackground worker: many messages vs one batched message~n", []),
    lists:foreach(fun(Count) -> print_result_batch(64, Count) end, ?RESULT_BATCH_COUNTS),
    ok.

warmup() ->
    _ = direct_ok_loop(1000, 0),
    _ = nif_send_ok_run(1000),
    _ = nif_owned_send_ok_run(1000),
    _ = erl_send_ok_run(1000),
    _ = direct_binary_loop(100, ?BIN_SIZE, 0),
    _ = nif_send_binary_run(100, ?BIN_SIZE),
    _ = nif_owned_send_binary_run(100, ?BIN_SIZE),
    _ = erl_send_binary_run(100, ?BIN_SIZE),
    _ = batch_write_run(1, ?BIN_SIZE, 64),
    _ = batch_read_run(1, ?BIN_SIZE, 64),
    _ = batch_mixed_run(2, ?BIN_SIZE, 64),
    _ = queue_batch_write_run(1, ?BIN_SIZE, 8),
    _ = queue_batch_read_run(1, ?BIN_SIZE, 8),
    _ = zero_read_bench(64, 1000),
    ok.

time(Fun) ->
    flush_mailbox(),
    {Micros, Value} = timer:tc(Fun),
    #{us => Micros, value => Value}.

direct_ok_loop(0, Acc) ->
    Acc;
direct_ok_loop(N, Acc) ->
    ok = ranch_uring_nif:bench_direct_ok(),
    direct_ok_loop(N - 1, Acc + 1).

direct_binary_loop(0, _Size, Acc) ->
    Acc;
direct_binary_loop(N, Size, Acc) ->
    {ok, Bin} = ranch_uring_nif:bench_direct_binary(Size),
    direct_binary_loop(N - 1, Size, Acc + byte_size(Bin)).

nif_send_ok_run(N) ->
    Self = self(),
    Sink = spawn_link(fun() -> sink_ok(Self, N, 0) end),
    Count = nif_send_ok_loop(N, Sink, 0),
    receive
        {bench_done, Sink, Count} -> Count
    after 10000 ->
        error(nif_send_ok_timeout)
    end.

nif_send_ok_loop(0, _Sink, Acc) ->
    Acc;
nif_send_ok_loop(N, Sink, Acc) ->
    ok = ranch_uring_nif:bench_env_send_ok(Sink),
    nif_send_ok_loop(N - 1, Sink, Acc + 1).

nif_owned_send_ok_run(N) ->
    Self = self(),
    Sink = spawn_link(fun() -> sink_ok(Self, N, 0) end),
    Count = nif_owned_send_ok_loop(N, Sink, 0),
    receive
        {bench_done, Sink, Count} -> Count
    after 10000 ->
        error(nif_owned_send_ok_timeout)
    end.

nif_owned_send_ok_loop(0, _Sink, Acc) ->
    Acc;
nif_owned_send_ok_loop(N, Sink, Acc) ->
    ok = ranch_uring_nif:bench_owned_env_send_ok(Sink),
    nif_owned_send_ok_loop(N - 1, Sink, Acc + 1).

erl_send_ok_run(N) ->
    Self = self(),
    Sink = spawn_link(fun() -> sink_ok(Self, N, 0) end),
    Count = erl_send_ok_loop(N, Sink, 0),
    receive
        {bench_done, Sink, Count} -> Count
    after 10000 ->
        error(erl_send_ok_timeout)
    end.

erl_send_ok_loop(0, _Sink, Acc) ->
    Acc;
erl_send_ok_loop(N, Sink, Acc) ->
    Sink ! ?REPLY_TAG_OK,
    erl_send_ok_loop(N - 1, Sink, Acc + 1).

nif_send_binary_run(N, Size) ->
    Self = self(),
    Sink = spawn_link(fun() -> sink_binary(Self, N, 0) end),
    Bytes = nif_send_binary_loop(N, Sink, Size),
    receive
        {bench_done, Sink, Bytes} -> Bytes
    after 10000 ->
        error(nif_send_binary_timeout)
    end.

nif_send_binary_loop(0, _Sink, _Size) ->
    0;
nif_send_binary_loop(N, Sink, Size) ->
    ok = ranch_uring_nif:bench_env_send_binary(Sink, Size),
    Size + nif_send_binary_loop(N - 1, Sink, Size).

nif_owned_send_binary_run(N, Size) ->
    Self = self(),
    Sink = spawn_link(fun() -> sink_binary(Self, N, 0) end),
    Bytes = nif_owned_send_binary_loop(N, Sink, Size),
    receive
        {bench_done, Sink, Bytes} -> Bytes
    after 10000 ->
        error(nif_owned_send_binary_timeout)
    end.

nif_owned_send_binary_loop(0, _Sink, _Size) ->
    0;
nif_owned_send_binary_loop(N, Sink, Size) ->
    ok = ranch_uring_nif:bench_owned_env_send_binary(Sink, Size),
    Size + nif_owned_send_binary_loop(N - 1, Sink, Size).

erl_send_binary_run(N, Size) ->
    Self = self(),
    Sink = spawn_link(fun() -> sink_binary(Self, N, 0) end),
    Bin = binary:copy(<<16#5A>>, Size),
    Bytes = erl_send_binary_loop(N, Sink, Bin, 0),
    receive
        {bench_done, Sink, Bytes} -> Bytes
    after 10000 ->
        error(erl_send_binary_timeout)
    end.

erl_send_binary_loop(0, _Sink, _Bin, Acc) ->
    Acc;
erl_send_binary_loop(N, Sink, Bin, Acc) ->
    Sink ! {?REPLY_TAG_BIN, Bin},
    erl_send_binary_loop(N - 1, Sink, Bin, Acc + byte_size(Bin)).

sink_ok(Parent, 0, Count) ->
    Parent ! {bench_done, self(), Count};
sink_ok(Parent, N, Count) ->
    receive
        ?REPLY_TAG_OK ->
            sink_ok(Parent, N - 1, Count + 1)
    end.

sink_binary(Parent, 0, Bytes) ->
    Parent ! {bench_done, self(), Bytes};
sink_binary(Parent, N, Bytes) ->
    receive
        {?REPLY_TAG_BIN, Bin} ->
            sink_binary(Parent, N - 1, Bytes + byte_size(Bin))
    end.

flush_mailbox() ->
    receive
        _ ->
            flush_mailbox()
    after 0 ->
        ok
    end.

print_result(Label, #{us := Micros}, Iterations) ->
    OpsPerSec = Iterations * 1000000 / Micros,
    NsPerOp = Micros * 1000 / Iterations,
    io:format("  ~ts: ~w ops/s, ~.1f ns/op~n", [Label, trunc(OpsPerSec), NsPerOp]).

print_batch(BatchSize) ->
    Iterations = max(1, ?BATCH_OPS div BatchSize),
    Write = time(fun() -> batch_write_run(Iterations, ?BIN_SIZE, BatchSize) end),
    Read = time(fun() -> batch_read_run(Iterations, ?BIN_SIZE, BatchSize) end),
    Mixed = time(fun() -> batch_mixed_run(Iterations, ?BIN_SIZE, BatchSize) end),
    io:format("  batch=~B~n", [BatchSize]),
    print_result("    write tuples", Write, Iterations * BatchSize),
    print_result("    read tuples", Read, Iterations * BatchSize),
    print_result("    mixed tuples", Mixed, Iterations * BatchSize).

print_queue_batch(Size) ->
    io:format("  payload=~B bytes~n", [Size]),
    lists:foreach(
      fun(BatchSize) ->
          Ops = queue_total_ops(Size, BatchSize),
          Iterations = max(1, Ops div BatchSize),
          Write = time(fun() -> queue_batch_write_run(Iterations, Size, BatchSize) end),
          Read = time(fun() -> queue_batch_read_run(Iterations, Size, BatchSize) end),
          io:format("    batch=~B~n", [BatchSize]),
          print_result("      queue write", Write, Iterations * BatchSize),
          print_result("      queue read", Read, Iterations * BatchSize),
          print_bandwidth("      queue write", Write#{value => Iterations * BatchSize * Size}),
          print_bandwidth("      queue read", Read#{value => Iterations * BatchSize * Size})
      end,
      ?QUEUE_BATCH_SIZES).

queue_total_ops(Size, _BatchSize) when Size =< 512 ->
    200000;
queue_total_ops(Size, _BatchSize) when Size =< 4096 ->
    50000;
queue_total_ops(Size, _BatchSize) when Size =< 16384 ->
    12000;
queue_total_ops(_Size, _BatchSize) ->
    4000.

zero_read_bench(Size, Iterations) ->
    Nif = time(fun() -> zero_read_nif_loop(Iterations, Size, 0) end),
    Erl = time(fun() -> zero_read_erl_loop(Iterations, Size, 0) end),
    File = time(fun() -> zero_read_file_loop(Iterations, Size) end),
    #{nif => Nif, erl => Erl, file => File}.

zero_read_ipc_bench(Size, Iterations) ->
    Self = self(),
    Sink = spawn_link(fun() -> sink_binary(Self, Iterations, 0) end),
    Nif = time(fun() -> zero_read_ipc_loop(Iterations, Sink, Size, 0) end),
    receive
        {bench_done, Sink, Bytes} -> Nif#{value => Bytes}
    after 30000 ->
        error(zero_read_ipc_timeout)
    end.

zero_read_owned_ipc_bench(Size, Iterations) ->
    Self = self(),
    Sink = spawn_link(fun() -> sink_binary(Self, Iterations, 0) end),
    Nif = time(fun() -> zero_read_owned_ipc_loop(Iterations, Sink, Size, 0) end),
    receive
        {bench_done, Sink, Bytes} -> Nif#{value => Bytes}
    after 30000 ->
        error(zero_read_owned_ipc_timeout)
    end.

zero_read_env_roundtrip_bench(Size, Iterations) ->
    time(fun() -> zero_read_env_roundtrip_loop(Iterations, Size, 0) end).

zero_read_owned_roundtrip_bench(Size, Iterations) ->
    time(fun() -> zero_read_owned_roundtrip_loop(Iterations, Size, 0) end).

result_batch_bench(Size, Count, Iterations) ->
    Many = time(fun() -> zero_many_roundtrip_loop(Iterations, Size, Count, 0) end),
    Batch = time(fun() -> zero_batch_roundtrip_loop(Iterations, Size, Count, 0) end),
    #{many => Many, batch => Batch}.

runtime_nop_bench(Iterations) ->
    time(fun() -> runtime_nop_loop(Iterations, 0) end).

runtime_nop_loop(0, Count) ->
    Count;
runtime_nop_loop(N, Count) ->
    {ok, RequestId} = ranch_uring_nif:bench_runtime_nop_async(),
    case wait_runtime_nop_reply(RequestId, 30000) of
        ok ->
            runtime_nop_loop(N - 1, Count + 1);
        {error, Reason} ->
            error({runtime_nop_failed, Reason})
    end.

wait_runtime_nop_reply(RequestId, Timeout) ->
    receive
        {reply, RequestId, ok} ->
            ok;
        {reply, RequestId, {error, Reason}} ->
            {error, Reason};
        {nif_results, Pairs} ->
            case lists:keyfind(RequestId, 1, Pairs) of
                {RequestId, ok} ->
                    ok;
                {RequestId, {error, Reason}} ->
                    {error, Reason};
                false ->
                    wait_runtime_nop_reply(RequestId, Timeout)
            end;
        _Other ->
            wait_runtime_nop_reply(RequestId, Timeout)
    after Timeout ->
        error(runtime_nop_timeout)
    end.

print_zero_read(Size) ->
    #{nif := Nif, erl := Erl, file := File} = zero_read_bench(Size, ?ZERO_READ_ITERS),
    io:format("  size=~B bytes~n", [Size]),
    print_result("    nif zero read", Nif, ?ZERO_READ_ITERS),
    print_result("    erl zero read", Erl, ?ZERO_READ_ITERS),
    print_result("    file zero read", File, ?ZERO_READ_ITERS),
    print_bandwidth("    nif zero read", Nif#{value => maps:get(value, Nif, 0)}),
    print_bandwidth("    erl zero read", Erl#{value => maps:get(value, Erl, 0)}),
    print_bandwidth("    file zero read", File#{value => maps:get(value, File, 0)}).

print_result_batch(Size, Count) ->
    Iterations = max(1000, 100000 div Count),
    #{many := Many, batch := Batch} = result_batch_bench(Size, Count, Iterations),
    Ops = Iterations * Count,
    Bytes = Ops * Size,
    io:format("  size=~B count=~B iters=~B~n", [Size, Count, Iterations]),
    print_result("    worker many msgs", Many, Ops),
    print_result("    worker one batch", Batch, Ops),
    print_bandwidth("    worker many msgs", Many#{value => Bytes}),
    print_bandwidth("    worker one batch", Batch#{value => Bytes}).

zero_read_nif_loop(0, _Size, Bytes) ->
    Bytes;
zero_read_nif_loop(N, Size, Bytes) ->
    {ok, Bin} = ranch_uring_nif:bench_zero_read(Size),
    zero_read_nif_loop(N - 1, Size, Bytes + byte_size(Bin)).

zero_read_ipc_loop(0, _Sink, _Size, Bytes) ->
    Bytes;
zero_read_ipc_loop(N, Sink, Size, Bytes) ->
    ok = ranch_uring_nif:bench_zero_env_send_binary(Sink, Size),
    zero_read_ipc_loop(N - 1, Sink, Size, Bytes + Size).

zero_read_owned_ipc_loop(0, _Sink, _Size, Bytes) ->
    Bytes;
zero_read_owned_ipc_loop(N, Sink, Size, Bytes) ->
    ok = ranch_uring_nif:bench_zero_owned_env_send_binary(Sink, Size),
    zero_read_owned_ipc_loop(N - 1, Sink, Size, Bytes + Size).

zero_read_env_roundtrip_loop(0, _Size, Bytes) ->
    Bytes;
zero_read_env_roundtrip_loop(N, Size, Bytes) ->
    ok = ranch_uring_nif:bench_zero_env_send_binary(self(), Size),
    receive
        {?REPLY_TAG_BIN, Bin} ->
            zero_read_env_roundtrip_loop(N - 1, Size, Bytes + byte_size(Bin))
    after 30000 ->
        error(zero_read_env_roundtrip_timeout)
    end.

zero_read_owned_roundtrip_loop(0, _Size, Bytes) ->
    Bytes;
zero_read_owned_roundtrip_loop(N, Size, Bytes) ->
    ok = ranch_uring_nif:bench_zero_owned_env_send_binary(self(), Size),
    receive
        {?REPLY_TAG_BIN, Bin} ->
            zero_read_owned_roundtrip_loop(N - 1, Size, Bytes + byte_size(Bin))
    after 30000 ->
        error(zero_read_owned_roundtrip_timeout)
    end.

zero_many_roundtrip_loop(0, _Size, _Count, Bytes) ->
    Bytes;
zero_many_roundtrip_loop(N, Size, Count, Bytes) ->
    ok = ranch_uring_nif:bench_zero_owned_env_send_many(self(), Size, Count),
    Received = recv_many_bins(Count, 0),
    zero_many_roundtrip_loop(N - 1, Size, Count, Bytes + Received).

zero_batch_roundtrip_loop(0, _Size, _Count, Bytes) ->
    Bytes;
zero_batch_roundtrip_loop(N, Size, Count, Bytes) ->
    ok = ranch_uring_nif:bench_zero_owned_env_send_batch(self(), Size, Count),
    Received = recv_batch_bins(0),
    zero_batch_roundtrip_loop(N - 1, Size, Count, Bytes + Received).

recv_many_bins(0, Bytes) ->
    Bytes;
recv_many_bins(N, Bytes) ->
    receive
        {?REPLY_TAG_BIN, Bin} ->
            recv_many_bins(N - 1, Bytes + byte_size(Bin))
    after 30000 ->
        error(zero_many_roundtrip_timeout)
    end.

recv_batch_bins(Bytes) ->
    receive
        {?REPLY_TAG_BATCH, Items} ->
            lists:foldl(fun({_Id, Bin}, Acc) -> Acc + byte_size(Bin) end, Bytes, Items)
    after 30000 ->
        error(zero_batch_roundtrip_timeout)
    end.


zero_read_erl_loop(0, _Size, Bytes) ->
    Bytes;
zero_read_erl_loop(N, Size, Bytes) ->
    Bin = binary:copy(<<0>>, Size),
    zero_read_erl_loop(N - 1, Size, Bytes + byte_size(Bin)).

zero_read_file_loop(Iterations, Size) ->
    {ok, Io} = file:open("/dev/zero", [read, binary, raw]),
    try
        zero_read_file_loop(Io, Iterations, Size, 0)
    after
        ok = file:close(Io)
    end.

zero_read_file_loop(_Io, 0, _Size, Bytes) ->
    Bytes;
zero_read_file_loop(Io, N, Size, Bytes) ->
    {ok, Bin} = file:read(Io, Size),
    zero_read_file_loop(Io, N - 1, Size, Bytes + byte_size(Bin)).

batch_write_run(Iterations, Size, BatchSize) ->
    Bin = binary:copy(<<16#5A>>, Size),
    Cmds = [{Id, ?OP_WRITE, Bin} || Id <- lists:seq(1, BatchSize)],
    batch_loop(Iterations, Cmds, write, 0).

batch_read_run(Iterations, Size, BatchSize) ->
    Cmds = [{Id, ?OP_READ, Size} || Id <- lists:seq(1, BatchSize)],
    batch_loop(Iterations, Cmds, read, 0).

batch_mixed_run(Iterations, Size, BatchSize) ->
    Bin = binary:copy(<<16#5A>>, Size),
    Cmds = [case Id rem 2 of
                0 -> {Id, ?OP_WRITE, Bin};
                1 -> {Id, ?OP_READ, Size}
            end || Id <- lists:seq(1, BatchSize)],
    batch_loop(Iterations, Cmds, mixed, 0).

batch_loop(0, _Cmds, _Kind, Acc) ->
    Acc;
batch_loop(N, Cmds, Kind, Acc) ->
    Replies = ranch_uring_nif:bench_batch(Cmds),
    batch_loop(N - 1, Cmds, Kind, Acc + batch_reply_score(Replies, Kind)).

batch_reply_score(Replies, Kind) ->
    lists:sum([reply_score(Reply, Kind) || Reply <- Replies]).

reply_score({_Id, ok, undefined}, write) ->
    1;
reply_score({_Id, ok, Bin}, read) when is_binary(Bin) ->
    byte_size(Bin);
reply_score({_Id, ok, undefined}, mixed) ->
    1;
reply_score({_Id, ok, Bin}, mixed) when is_binary(Bin) ->
    byte_size(Bin);
reply_score(_, _) ->
    error(bad_bench_reply).

queue_batch_write_run(0, _Size, _BatchSize) ->
    0;
queue_batch_write_run(Iterations, Size, BatchSize) ->
    Bin = binary:copy(<<16#5A>>, Size),
    Cmds = [{Id, ?OP_WRITE, Bin} || Id <- lists:seq(1, BatchSize)],
    queue_batch_loop(Iterations, Cmds, write, 0).

queue_batch_read_run(0, _Size, _BatchSize) ->
    0;
queue_batch_read_run(Iterations, Size, BatchSize) ->
    Cmds = [{Id, ?OP_READ, Size} || Id <- lists:seq(1, BatchSize)],
    queue_batch_loop(Iterations, Cmds, read, 0).

queue_batch_loop(0, _Cmds, _Kind, Acc) ->
    Acc;
queue_batch_loop(N, Cmds, Kind, Acc) ->
    {ok, RequestId} = ranch_uring_nif:bench_queue_batch_async(Cmds),
    Replies = await_queue_batch_reply(RequestId, 30000),
    queue_batch_loop(N - 1, Cmds, Kind, Acc + queue_batch_reply_score(Replies, Kind)).

await_queue_batch_reply(RequestId, Timeout) ->
    receive
        {?REPLY_TAG_QUEUE, RequestId, Results} ->
            Results
    after Timeout ->
        error(queue_batch_timeout)
    end.

queue_batch_reply_score(Replies, Kind) ->
    lists:sum([reply_score_queue(Reply, Kind) || Reply <- Replies]).

reply_score_queue({_Id, ok}, write) ->
    1;
reply_score_queue({_Id, {ok, Bin}}, read) when is_binary(Bin) ->
    byte_size(Bin);
reply_score_queue(_, _) ->
    error(bad_queue_bench_reply).

print_bandwidth(Label, #{us := Micros, value := Bytes}) ->
    MBps = Bytes / Micros,
    io:format("~ts bandwidth: ~.1f MB/s~n", [Label, MBps]).

queue_write_bench(Size, BatchSize, Iterations)
        when is_integer(Size), Size >= 0, is_integer(BatchSize), BatchSize > 0,
             is_integer(Iterations), Iterations > 0 ->
    Write = time(fun() -> queue_batch_write_run(Iterations, Size, BatchSize) end),
    Ops = Iterations * BatchSize,
    Bytes = Ops * Size,
    io:format("queue write payload=~B batch=~B iterations=~B~n", [Size, BatchSize, Iterations]),
    print_result("  queue write", Write, Ops),
    print_bandwidth("  queue write", Write#{value => Bytes}),
    ok.

queue_read_bench(Size, BatchSize, Iterations)
        when is_integer(Size), Size >= 0, is_integer(BatchSize), BatchSize > 0,
             is_integer(Iterations), Iterations > 0 ->
    Read = time(fun() -> queue_batch_read_run(Iterations, Size, BatchSize) end),
    Ops = Iterations * BatchSize,
    Bytes = Ops * Size,
    io:format("queue read payload=~B batch=~B iterations=~B~n", [Size, BatchSize, Iterations]),
    print_result("  queue read", Read, Ops),
    print_bandwidth("  queue read", Read#{value => Bytes}),
    ok.
