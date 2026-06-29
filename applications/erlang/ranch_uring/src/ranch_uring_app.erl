-module(ranch_uring_app).
-behaviour(application).

-export([start/2, stop/1]).

start(_StartType, _StartArgs) ->
    case ensure_nif_loaded() of
        ok ->
            ranch_uring_sup:start_link();
        {error, Reason} ->
            fatal_startup_error(Reason)
    end.

stop(_State) ->
    case ranch_uring_nif:stop_runtime() of
        ok ->
            ok;
        {error, closed} ->
            ok;
        {error, not_loaded} ->
            ok;
        {error, _Reason} ->
            ok
    end.

ensure_nif_loaded() ->
    try ranch_uring_nif:init() of
        ok ->
            ensure_nif_ready();
        {error, _} = Error ->
            Error;
        Other ->
            {error, {unexpected_nif_init_result, Other}}
    catch
        error:not_loaded ->
            {error, not_ready};
        error:Reason ->
            {error, Reason};
        exit:Reason ->
            {error, {nif_init_exit, Reason}}
    end.

ensure_nif_ready() ->
    try ranch_uring_nif:ready() of
        ok ->
            ok;
        {error, _} = Error ->
            Error;
        Other ->
            {error, {unexpected_nif_ready_result, Other}}
    catch
        error:not_loaded ->
            {error, not_ready};
        error:Reason ->
            {error, Reason};
        exit:Reason ->
            {error, {nif_ready_exit, Reason}}
    end.

fatal_startup_error(Reason) ->
    exit({ranch_uring_startup_failed, Reason}).
