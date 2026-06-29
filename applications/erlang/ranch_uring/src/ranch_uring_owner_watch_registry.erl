-module(ranch_uring_owner_watch_registry).
-behaviour(gen_server).

-export([start_link/0, ensure_table/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).

-define(OWNER_WATCH_TABLE, ranch_uring_owner_watch).

start_link() ->
    gen_server:start_link({local, ?MODULE}, ?MODULE, [], []).

ensure_table() ->
    gen_server:call(?MODULE, ensure_table).

init([]) ->
    ensure_owner_watch_table(),
    {ok, #{}}.

handle_call(ensure_table, _From, State) ->
    ensure_owner_watch_table(),
    {reply, ok, State};
handle_call(_Request, _From, State) ->
    {reply, ok, State}.

handle_cast(_Msg, State) ->
    {noreply, State}.

handle_info(_Info, State) ->
    {noreply, State}.

terminate(_Reason, _State) ->
    ok.

code_change(_OldVsn, State, _Extra) ->
    {ok, State}.

ensure_owner_watch_table() ->
    case ets:info(?OWNER_WATCH_TABLE) of
        undefined ->
            _ = ets:new(
                ?OWNER_WATCH_TABLE,
                [named_table, public, set, {read_concurrency, true}, {write_concurrency, true}]
            ),
            ok;
        _ ->
            ok
    end.
