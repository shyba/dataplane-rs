-module(ranch_uring_demux).
-behaviour(gen_server).

-export([start_link/0, start_pool/1, stop_pool/0, demuxes/0]).
-export([submit_stat/1, submit_stat_wait/1, submit_accept/2, submit_recv/2, await/2, cancel_wait/2, subscribe/2, await_subscription/2]).

-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).

-record(state, {
    pending = #{} :: map(),
    waiters = #{} :: map(),
    canceled = #{} :: map(),
    sub_waiters = #{} :: map(),
    sub_next = #{} :: map(),
    sub_pop = #{} :: map(),
    sub_tab :: ets:tid()
}).

start_link() ->
    gen_server:start_link(?MODULE, [], []).

start_pool(N) when is_integer(N), N > 0 ->
    case ranch_uring_sup:start_pool(N) of
        {ok, Demuxes} ->
            {ok, Demuxes};
        {error, _} = Error ->
            Error
    end.

stop_pool() ->
    ranch_uring_sup:stop_pool().

demuxes() ->
    ranch_uring_sup:demuxes().

submit_stat(Fd) ->
    case pick_demux() of
        {ok, DemuxPid} ->
            case ranch_uring_nif:stat_async_to(Fd, DemuxPid) of
                {ok, ReqId} ->
                    {ok, {DemuxPid, ReqId}};
                Error ->
                    Error
            end;
        Error ->
            Error
    end.

submit_stat_wait(Fd) ->
    case pick_demux() of
        {ok, DemuxPid} ->
            case ranch_uring_nif:stat_async_to(Fd, DemuxPid) of
                {ok, ReqId} ->
                    case register_waiter_call(DemuxPid, ReqId, self()) of
                        ok ->
                            {ok, ReqId};
                        Error ->
                            Error
                    end;
                Error ->
                    Error
            end;
        Error ->
            Error
    end.

submit_accept(ListenRef, TimeoutMs) ->
    case pick_demux() of
        {ok, DemuxPid} ->
            case ranch_uring_nif:accept_async_to(ListenRef, TimeoutMs, DemuxPid) of
                {ok, ReqId} ->
                    {ok, {DemuxPid, ReqId}};
                Error ->
                    Error
            end;
        Error ->
            Error
    end.

submit_recv(Socket, Length) ->
    case pick_demux() of
        {ok, DemuxPid} ->
            case ranch_uring_nif:recv_async_to(Socket, Length, DemuxPid) of
                {ok, ReqId} ->
                    {ok, {DemuxPid, ReqId}};
                Error ->
                    Error
            end;
        Error ->
            Error
    end.

await({DemuxPid, ReqId}, TimeoutMs)
when is_pid(DemuxPid), is_integer(ReqId), is_integer(TimeoutMs), TimeoutMs >= 0 ->
    Deadline = deadline_after(TimeoutMs),
    MonRef = erlang:monitor(process, DemuxPid),
    case register_waiter_call(DemuxPid, ReqId, self(), TimeoutMs) of
        ok ->
            Result = await_reply_deadline(MonRef, DemuxPid, ReqId, Deadline),
            _ = erlang:demonitor(MonRef, [flush]),
            Result;
        timeout ->
            _ = cancel_wait_best_effort({DemuxPid, ReqId}, self()),
            _ = erlang:demonitor(MonRef, [flush]),
            timeout;
        Error ->
            _ = erlang:demonitor(MonRef, [flush]),
            Error
    end;
await({DemuxPid, ReqId}, infinity)
when is_pid(DemuxPid), is_integer(ReqId) ->
    MonRef = erlang:monitor(process, DemuxPid),
    case register_waiter_call(DemuxPid, ReqId, self()) of
        ok ->
            Result = await_reply_infinity(MonRef, DemuxPid, ReqId),
            _ = erlang:demonitor(MonRef, [flush]),
            Result;
        Error ->
            _ = erlang:demonitor(MonRef, [flush]),
            Error
    end.

register_waiter_call(DemuxPid, ReqId, Pid) ->
    register_waiter_call(DemuxPid, ReqId, Pid, infinity).

register_waiter_call(DemuxPid, ReqId, Pid, Timeout) ->
    try gen_server:call(DemuxPid, {register_waiter, ReqId, Pid}, Timeout) of
        ok ->
            ok
    catch
        exit:{timeout, _} ->
            timeout;
        exit:{noproc, _} ->
            {error, closed};
        exit:_ ->
            {error, closed}
    end.

cancel_wait({DemuxPid, ReqId}, CallerPid) when is_pid(DemuxPid), is_integer(ReqId), is_pid(CallerPid) ->
    cancel_wait({DemuxPid, ReqId}, CallerPid, infinity);
cancel_wait(_, _) ->
    {error, badarg}.

cancel_wait({DemuxPid, ReqId}, CallerPid, Timeout) when is_pid(DemuxPid), is_integer(ReqId), is_pid(CallerPid) ->
    try gen_server:call(DemuxPid, {cancel_waiter, ReqId, CallerPid}, Timeout) of
        ok ->
            ok
    catch
        exit:{timeout, _} ->
            timeout;
        exit:{noproc, _} ->
            {error, closed};
        exit:_ ->
            {error, closed}
    end;
cancel_wait(_, _, _) ->
    {error, badarg}.

cancel_wait_best_effort(Future, CallerPid) ->
    _ = cancel_wait(Future, CallerPid, 1),
    ok.

deadline_after(TimeoutMs) ->
    erlang:monotonic_time(millisecond) + TimeoutMs.

deadline_remaining(Deadline) ->
    erlang:max(0, Deadline - erlang:monotonic_time(millisecond)).

await_reply_deadline(MonRef, DemuxPid, ReqId, Deadline) ->
    Remaining = deadline_remaining(Deadline),
    receive
        {'DOWN', MonRef, process, DemuxPid, _Reason} ->
            {error, closed};
        {nif_result, ReqId, Result} ->
            Result
    after Remaining ->
            case cancel_wait({DemuxPid, ReqId}, self(), 1) of
                ok ->
                    receive_late_result(ReqId);
                timeout ->
                    receive_late_result(ReqId);
                {error, closed} ->
                    {error, closed};
                _ ->
                    timeout
            end
    end.

await_reply_infinity(MonRef, DemuxPid, ReqId) ->
    receive
        {'DOWN', MonRef, process, DemuxPid, _Reason} ->
            {error, closed};
        {nif_result, ReqId, Result} ->
            Result
    end.

receive_late_result(ReqId) ->
    receive
        {nif_result, ReqId, Result} ->
            Result
    after 0 ->
            timeout
    end.

subscribe(Operation, Fd) ->
    case pick_demux() of
        {ok, DemuxPid} ->
            case ranch_uring_nif:command({subscribe, DemuxPid, Operation, Fd}) of
                {ok, SubscriptionId} ->
                    ok = gen_server:call(DemuxPid, {register_subscription, SubscriptionId}, 5000),
                    {ok, {DemuxPid, SubscriptionId}};
                Error ->
                    Error
            end;
        Error ->
            Error
    end.

await_subscription({DemuxPid, SubscriptionId}, TimeoutMs)
when is_pid(DemuxPid), is_integer(SubscriptionId), is_integer(TimeoutMs), TimeoutMs >= 0 ->
    try gen_server:call(DemuxPid, {await_subscription, SubscriptionId}, TimeoutMs) of
        Reply ->
            Reply
    catch
        exit:{timeout, _} ->
            timeout;
        exit:{noproc, _} ->
            {error, closed};
        exit:_ ->
            {error, closed}
    end;
await_subscription({DemuxPid, SubscriptionId}, infinity)
when is_pid(DemuxPid), is_integer(SubscriptionId) ->
    try gen_server:call(DemuxPid, {await_subscription, SubscriptionId}, infinity) of
        Reply ->
            Reply
    catch
        exit:{noproc, _} ->
            {error, closed};
        exit:_ ->
            {error, closed}
    end.

init([]) ->
    Tab = ets:new(?MODULE, [ordered_set, private, {read_concurrency, true}, {write_concurrency, true}]),
    {ok, #state{sub_tab = Tab}}.

handle_call({await, ReqId}, From, State0 = #state{pending = Pending0, waiters = Waiters0}) ->
    case maps:take(ReqId, Pending0) of
        {Result, Pending1} ->
            {reply, Result, State0#state{pending = Pending1}};
        error ->
            Waiters1 = waiters_put(ReqId, {call, From}, Waiters0),
            {noreply, State0#state{waiters = Waiters1}}
    end;
handle_call({register_subscription, SubscriptionId}, _From, State0) ->
    State1 = ensure_subscription(SubscriptionId, State0),
    {reply, ok, State1};
handle_call({await_subscription, SubscriptionId}, From, State0) ->
    State1 = ensure_subscription(SubscriptionId, State0),
    case sub_pop(SubscriptionId, State1) of
        {ok, Result, State2} ->
            {reply, Result, State2};
        empty ->
            SubWaiters0 = State1#state.sub_waiters,
            SubWaiters1 = sub_waiters_put(SubscriptionId, From, SubWaiters0),
            {noreply, State1#state{sub_waiters = SubWaiters1}}
    end;
handle_call({register_waiter, ReqId, Pid}, _From, State0) when is_integer(ReqId), is_pid(Pid) ->
    {reply, ok, register_waiter(ReqId, {pid, Pid}, State0)};
handle_call({cancel_waiter, ReqId, Pid}, _From, State0) when is_integer(ReqId), is_pid(Pid) ->
    {reply, ok, cancel_waiter(ReqId, Pid, State0)};
handle_call(_Req, _From, State) ->
    {reply, {error, badarg}, State}.
handle_cast(_Msg, State) ->
    {noreply, State}.

handle_info({reply, ReqId, Result}, State0) when is_integer(ReqId) ->
    {noreply, route_result(ReqId, Result, State0)};
handle_info({nif_results, Pairs}, State0) when is_list(Pairs) ->
    State1 = lists:foldl(
        fun
            ({ReqId, Result}, Acc) when is_integer(ReqId) ->
                route_result(ReqId, Result, Acc);
            (_, Acc) ->
                Acc
        end,
        State0,
        Pairs
    ),
    {noreply, State1};
handle_info(_Info, State) ->
    {noreply, State}.

terminate(_Reason, _State) ->
    ok.

code_change(_OldVsn, State, _Extra) ->
    {ok, State}.

pick_demux() ->
    Demuxes = demuxes(),
    case Demuxes of
        [] ->
            {error, no_demux};
        _ ->
            N = length(Demuxes),
            Index0 =
                case ranch_uring_nif:caller_shard() of
                    {ok, Shard} when is_integer(Shard), Shard >= 0 ->
                        Shard rem N;
                    _ ->
                        erlang:phash2(self(), N)
                end,
            {ok, lists:nth(Index0 + 1, Demuxes)}
    end.

waiters_put(ReqId, Waiter, Waiters0) ->
    WaiterList0 = maps:get(ReqId, Waiters0, []),
    Waiters0#{ReqId => [Waiter | WaiterList0]}.

cancel_waiter(ReqId, Pid, State0 = #state{pending = Pending0, waiters = Waiters0, canceled = Canceled0}) ->
    case maps:take(ReqId, Waiters0) of
        {Waiters, Waiters1} ->
            Rest = lists:filter(
                fun
                    ({pid, ActivePid}) when ActivePid =:= Pid -> false;
                    (_) -> true
                end,
                Waiters
            ),
            Waiters2 =
                case Rest of
                    [] -> Waiters1;
                    _ -> Waiters1#{ReqId => Rest}
                end,
            State0#state{waiters = Waiters2, pending = maps:remove(ReqId, Pending0), canceled = Canceled0#{ReqId => true}};
        error ->
            State0#state{pending = maps:remove(ReqId, Pending0), canceled = Canceled0#{ReqId => true}}
    end.

register_waiter(ReqId, Waiter, State0 = #state{pending = Pending0, waiters = Waiters0, canceled = Canceled0}) ->
    case maps:take(ReqId, Pending0) of
        {Result, Pending1} ->
            deliver_waiter(Waiter, ReqId, Result),
            State0#state{pending = Pending1, canceled = maps:remove(ReqId, Canceled0)};
        error ->
            case maps:is_key(ReqId, Canceled0) of
                true ->
                    State0#state{canceled = maps:remove(ReqId, Canceled0)};
                false ->
                    Waiters1 = waiters_put(ReqId, Waiter, Waiters0),
                    State0#state{waiters = Waiters1}
            end
    end.

sub_waiters_put(SubId, From, SubWaiters0) ->
    WaiterList0 = maps:get(SubId, SubWaiters0, []),
    SubWaiters0#{SubId => [From | WaiterList0]}.

ensure_subscription(SubId, State0 = #state{sub_next = Next0, sub_pop = Pop0}) ->
    Next1 =
        case maps:is_key(SubId, Next0) of
            true -> Next0;
            false -> Next0#{SubId => 0}
        end,
    Pop1 =
        case maps:is_key(SubId, Pop0) of
            true -> Pop0;
            false -> Pop0#{SubId => 0}
        end,
    State0#state{sub_next = Next1, sub_pop = Pop1}.

route_result(ReqId, Result, State0 = #state{sub_next = SubNext0}) ->
    case maps:is_key(ReqId, SubNext0) of
        true ->
            route_subscription_result(ReqId, Result, State0);
        false ->
            route_request_result(ReqId, Result, State0)
    end.

route_request_result(
    ReqId,
    Result,
    State0 = #state{waiters = Waiters0, pending = Pending0, canceled = Canceled0}
) ->
    case maps:take(ReqId, Waiters0) of
        {[], Waiters1} ->
            case maps:is_key(ReqId, Canceled0) of
                true ->
                    State0#state{waiters = Waiters1, canceled = maps:remove(ReqId, Canceled0)};
                false ->
                    State0#state{waiters = Waiters1, pending = Pending0#{ReqId => Result}}
            end;
        {[Waiter | Rest], Waiters1} ->
            deliver_waiter(Waiter, ReqId, Result),
            Waiters2 =
                case Rest of
                    [] -> Waiters1;
                    _ -> Waiters1#{ReqId => Rest}
                end,
            State0#state{waiters = Waiters2, canceled = maps:remove(ReqId, Canceled0)};
        error ->
            case maps:is_key(ReqId, Canceled0) of
                true ->
                    State0#state{canceled = maps:remove(ReqId, Canceled0)};
                false ->
                    State0#state{pending = Pending0#{ReqId => Result}}
            end
    end.

deliver_waiter({call, From}, _ReqId, Result) ->
    gen_server:reply(From, Result);
deliver_waiter({pid, Pid}, ReqId, Result) ->
    Pid ! {nif_result, ReqId, Result};
deliver_waiter(_, _ReqId, _Result) ->
    ok.

route_subscription_result(SubId, Result, State0) ->
    State1 = sub_push(SubId, Result, State0),
    maybe_flush_sub_waiters(SubId, State1).

sub_push(SubId, Result, State0 = #state{sub_next = Next0, sub_tab = Tab}) ->
    Seq = maps:get(SubId, Next0, 0),
    true = ets:insert(Tab, {{SubId, Seq}, Result}),
    State0#state{sub_next = Next0#{SubId => Seq + 1}}.

sub_pop(SubId, State0 = #state{sub_pop = Pop0, sub_tab = Tab}) ->
    Seq = maps:get(SubId, Pop0, 0),
    case ets:take(Tab, {SubId, Seq}) of
        [{{SubId, Seq}, Result}] ->
            Pop1 = Pop0#{SubId => Seq + 1},
            {ok, Result, State0#state{sub_pop = Pop1}};
        [] ->
            empty
    end.

maybe_flush_sub_waiters(SubId, State0 = #state{sub_waiters = SubWaiters0}) ->
    case maps:get(SubId, SubWaiters0, []) of
        [] ->
            State0;
        Waiters0 ->
            flush_sub_waiters(SubId, lists:reverse(Waiters0), State0#state{sub_waiters = maps:remove(SubId, SubWaiters0)})
    end.

flush_sub_waiters(_SubId, [], State) ->
    State;
flush_sub_waiters(SubId, [From | Rest], State0) ->
    case sub_pop(SubId, State0) of
        {ok, Result, State1} ->
            gen_server:reply(From, Result),
            flush_sub_waiters(SubId, Rest, State1);
        empty ->
            sub_waiters_restore(SubId, [From | Rest], State0)
    end.

sub_waiters_restore(SubId, Waiters, State0 = #state{sub_waiters = SubWaiters0}) ->
    Existing = maps:get(SubId, SubWaiters0, []),
    State0#state{sub_waiters = SubWaiters0#{SubId => lists:reverse(Waiters) ++ Existing}}.
