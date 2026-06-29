-module(ranch_uring_sup).
-behaviour(supervisor).

-export([start_link/0, init/1, start_pool/1, stop_pool/0, demuxes/0]).

start_link() ->
    supervisor:start_link({local, ?MODULE}, ?MODULE, []).

init([]) ->
    case build_demux_children() of
        {ok, DemuxChildren} ->
            SupFlags = #{
                strategy => one_for_one,
                intensity => 1,
                period => 5
            },
            OwnerWatchRegistry = #{
                id => ranch_uring_owner_watch_registry,
                start => {ranch_uring_owner_watch_registry, start_link, []},
                restart => permanent,
                shutdown => 5000,
                type => worker,
                modules => [ranch_uring_owner_watch_registry]
            },
            {ok, {SupFlags, [OwnerWatchRegistry | DemuxChildren]}};
        {error, _Reason} ->
            {stop, {demux_pool_start_failed, _Reason}}
    end.

start_pool(N) when is_integer(N), N > 0 ->
    case ensure_supervisor_pid() of
        {ok, SupPid} ->
            case demuxes(SupPid) of
                Demuxes when length(Demuxes) =:= N ->
                    {ok, Demuxes};
                _ ->
                    _ = stop_pool(SupPid),
                    start_pool_children(SupPid, 1, N, [])
            end;
        Error ->
            Error
    end.

stop_pool() ->
    stop_pool(whereis(?MODULE)),
    ok.

demuxes() ->
    case ensure_supervisor_pid() of
        {ok, SupPid} ->
            demuxes(SupPid);
        {error, supervisor_not_running} ->
            []
    end.

demuxes(SupPid) when is_pid(SupPid) ->
    case erlang:is_process_alive(SupPid) of
        true ->
            Children = supervisor:which_children(SupPid),
            DemuxChildren = [
                {Id, Pid} ||
                    {Id = {ranch_uring_demux, _}, Pid, _Type, _Modules} <- Children,
                    is_pid(Pid)
            ],
            Sorted = lists:sort(fun({{_, A}, _}, {{_, B}, _}) -> A < B end, DemuxChildren),
            [Pid || {_Id, Pid} <- Sorted];
        false ->
            []
    end;
demuxes(_) ->
    [].

build_demux_children() ->
    try
        Count = demux_worker_count(),
        {ok, [demux_child_spec(Index) || Index <- lists:seq(1, Count)]}
    catch
        error:{badarg, _Reason} ->
            {error, badarg};
        error:Reason ->
            {error, Reason}
    end.

start_pool_children(_SupPid, Index, Max, Acc) when Index > Max ->
    {ok, lists:reverse(Acc)};
start_pool_children(SupPid, Index, Max, Acc) ->
    Spec = demux_child_spec(Index),
    case supervisor:start_child(SupPid, Spec) of
        {ok, Pid} ->
            start_pool_children(SupPid, Index + 1, Max, [Pid | Acc]);
        {ok, Pid, _} ->
            start_pool_children(SupPid, Index + 1, Max, [Pid | Acc]);
        {error, _Reason} = Error ->
            _ = stop_pool(SupPid),
            Error
    end.

stop_pool(SupPid) when is_pid(SupPid) ->
    true = erlang:is_process_alive(SupPid),
    ChildIds = [Id || {Id = {ranch_uring_demux, _}, _Pid, _Type, _Modules} <- supervisor:which_children(SupPid)],
    lists:foreach(
        fun(ChildId) ->
            catch supervisor:terminate_child(SupPid, ChildId),
            catch supervisor:delete_child(SupPid, ChildId)
        end,
        ChildIds
    );
stop_pool(_) ->
    ok.

ensure_supervisor_pid() ->
    case whereis(?MODULE) of
        undefined ->
            {error, supervisor_not_running};
        SupPid when is_pid(SupPid) ->
            case erlang:is_process_alive(SupPid) of
                true -> {ok, SupPid};
                false -> {error, supervisor_not_running}
            end
    end.

demux_worker_count() ->
    MaxShards = 256,
    case os:getenv("RANCH_URING_SHARDS") of
        false ->
            erlang:system_info(schedulers_online);
        Value ->
            case parse_positive_integer(Value) of
                {ok, Count} ->
                    erlang:max(1, erlang:min(Count, MaxShards));
                error ->
                    erlang:system_info(schedulers_online)
            end
    end.

parse_positive_integer(Value) when is_list(Value) ->
    try
        Count = list_to_integer(string:trim(Value)),
        case Count of
            Count when Count > 0 -> {ok, Count};
            _ -> error
        end
    catch
        error:badarg -> error
    end;
parse_positive_integer(_) ->
    error.

demux_child_spec(Index) ->
    #{
        id => {ranch_uring_demux, Index},
        start => {ranch_uring_demux, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [ranch_uring_demux]
    }.
