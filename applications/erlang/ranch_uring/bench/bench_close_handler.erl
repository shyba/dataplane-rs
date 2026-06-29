-module(bench_close_handler).
-behaviour(ranch_protocol).

-export([start_link/3]).
-export([init/3]).

start_link(Ref, Transport, Opts) ->
    SpawnOpts = case mqd_opt(Opts) of
        off_heap -> [{message_queue_data, off_heap}];
        on_heap -> [{message_queue_data, on_heap}];
        _ -> []
    end,
    Pid = spawn_opt(?MODULE, init, [Ref, Transport, Opts], [link | SpawnOpts]),
    {ok, Pid}.

mqd_opt(Opts) when is_map(Opts) ->
    maps:get(message_queue_data, Opts, off_heap);
mqd_opt(Opts) ->
    proplists:get_value(message_queue_data, Opts, off_heap).

init(Ref, Transport, _Opts) ->
    {ok, Socket} = ranch:handshake(Ref),
    _ = Transport:close(Socket),
    ok.
