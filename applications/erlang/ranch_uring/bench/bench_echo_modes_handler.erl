-module(bench_echo_modes_handler).
-behaviour(ranch_protocol).

-export([start_link/3, init/3]).

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

init(Ref, Transport, Opts) ->
    {ok, Socket} = ranch:handshake(Ref),
    Mode = proplists:get_value(mode, Opts, passive),
    Payload = proplists:get_value(payload, Opts, <<>>),
    case Mode of
        passive ->
            passive_loop(Socket, Transport);
        sendv ->
            sendv_loop(Socket, Transport);
        batch ->
            batch_loop(Socket, Transport, Payload);
        active ->
            Transport:setopts(Socket, [{active, true}]),
            {Ok, Closed, Error, _Passive} = Transport:messages(),
            active_loop(Socket, Transport, Ok, Closed, Error)
    end.

passive_loop(Socket, Transport) ->
    case Transport:recv(Socket, 0, 5000) of
        {ok, Data} ->
            Transport:send(Socket, Data),
            passive_loop(Socket, Transport);
        {error, closed} ->
            ok;
        {error, _} ->
            ok
    end.

sendv_loop(Socket, Transport) ->
    case Transport:recv(Socket, 0, 5000) of
        {ok, Data} ->
            Parts = split_binary_parts(Data, 4),
            Result = case erlang:function_exported(Transport, sendv, 2) of
                true -> Transport:sendv(Socket, Parts);
                false -> Transport:send(Socket, Parts)
            end,
            case Result of
                ok -> sendv_loop(Socket, Transport);
                {error, _} -> ok
            end;
        {error, closed} ->
            ok;
        {error, _} ->
            ok
    end.

batch_loop(Socket, Transport, Payload) ->
    BatchResult = case erlang:function_exported(Transport, batch, 3) of
        true ->
            Transport:batch(Socket, [{1, read, 0}, {2, writev, Payload}], 30000);
        false ->
            ranch_uring:batch(Socket, [{1, read, 0}, {2, writev, Payload}], 30000)
    end,
    case BatchResult of
        {ok, [{1, {ok, _Data}}, {2, ok}]} ->
            batch_loop(Socket, Transport, Payload);
        {ok, _Other} ->
            ok;
        {error, closed} ->
            ok;
        {error, _} ->
            ok
    end.

active_loop(Socket, Transport, Ok, Closed, Error) ->
    receive
        {Ok, Socket, Data} ->
            Transport:send(Socket, Data),
            active_loop(Socket, Transport, Ok, Closed, Error);
        {Closed, Socket} ->
            ok;
        {Error, Socket, _Reason} ->
            ok
    end.

split_binary_parts(Data, Parts) ->
    Size = byte_size(Data),
    split_binary_parts(Data, Parts, Size div Parts, Size rem Parts, []).

split_binary_parts(<<>>, 0, _Base, _Extra, Acc) ->
    lists:reverse(Acc);
split_binary_parts(Data, N, Base, Extra, Acc) when N > 0 ->
    ThisSize = Base + if Extra > 0 -> 1; true -> 0 end,
    <<Part:ThisSize/binary, Rest/binary>> = Data,
    split_binary_parts(Rest, N - 1, Base, max(0, Extra - 1), [Part | Acc]).
