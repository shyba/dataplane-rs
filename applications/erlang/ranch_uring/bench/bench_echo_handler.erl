-module(bench_echo_handler).
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
    loop(Socket, Transport).

loop(Socket, Transport) ->
    case Transport:recv(Socket, 0, 5000) of
        {ok, Data} ->
            Transport:send(Socket, Data),
            loop(Socket, Transport);
        {error, closed} ->
            ok;
        {error, _} ->
            ok
    end.
