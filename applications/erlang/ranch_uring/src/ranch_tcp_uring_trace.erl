-module(ranch_tcp_uring_trace).

-export([start/1, stop/0, enabled/0, event/2]).

-define(KEY, {?MODULE, pid}).

start(Path) ->
    stop(),
    Parent = self(),
    Pid = spawn(fun() -> init(Path, Parent) end),
    receive
        {trace_started, Pid, ok} ->
            persistent_term:put(?KEY, Pid),
            ok;
        {trace_started, Pid, Error} ->
            Error
    end.

stop() ->
    case persistent_term:get(?KEY, undefined) of
        undefined ->
            ok;
        Pid ->
            Pid ! {stop, self()},
            receive
                {trace_stopped, Pid} ->
                    persistent_term:erase(?KEY),
                    ok
            after 5000 ->
                persistent_term:erase(?KEY),
                ok
            end
    end.

enabled() ->
    persistent_term:get(?KEY, undefined) =/= undefined.

event(Name, Fields) ->
    case persistent_term:get(?KEY, undefined) of
        undefined ->
            ok;
        Pid ->
            Pid ! {event, erlang:monotonic_time(microsecond), Name, Fields},
            ok
    end.

init(Path, Parent) ->
    Result = file:open(Path, [write, raw, binary, delayed_write]),
    Parent ! {trace_started, self(), case Result of {ok, _Fd} -> ok; Error -> Error end},
    case Result of
        {ok, Fd} -> loop(Fd);
        _ -> ok
    end.

loop(Fd) ->
    receive
        {event, Ts, Name, Fields} ->
            ok = file:write(Fd, encode_line(Ts, Name, Fields)),
            loop(Fd);
        {stop, From} ->
            file:close(Fd),
            From ! {trace_stopped, self()},
            ok;
        _Other ->
            loop(Fd)
    end.

encode_line(Ts, Name, Fields) ->
    [
        "ts=", integer_to_list(Ts),
        $\t,
        "event=", atom_to_list(Name),
        [[ $\t, atom_to_list(Key), $=, value_to_iolist(Value)] || {Key, Value} <- Fields],
        $\n
    ].

value_to_iolist(Value) when is_integer(Value) ->
    integer_to_list(Value);
value_to_iolist(Value) when is_atom(Value) ->
    atom_to_list(Value);
value_to_iolist(true) ->
    "true";
value_to_iolist(false) ->
    "false";
value_to_iolist(undefined) ->
    "undefined";
value_to_iolist(Value) when is_binary(Value) ->
    Value;
value_to_iolist(Value) ->
    io_lib:format("~p", [Value]).
