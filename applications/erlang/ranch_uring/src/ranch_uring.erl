-module(ranch_uring).
-behaviour(ranch_transport).

-export([name/0, secure/0, messages/0]).
-export([listen/1, accept/2, handshake/2, handshake/3]).
-export([handshake_continue/2, handshake_continue/3, handshake_cancel/1]).
-export([recv/3, send/2, sendfile/2, sendfile/4, sendfile/5]).
-export([batch/2, batch/3, sendv/2]).
-export([setopts/2, getopts/2, getstat/1, getstat/2]).
-export([controlling_process/2, peername/1, sockname/1, shutdown/2, close/1]).
-export([connect/3, connect/4, recv_proxy_header/2, cleanup/1]).

name() ->
    ranch_uring.

secure() ->
    ranch_tcp_uring:secure().

messages() ->
    ranch_tcp_uring:messages().

listen(TransOpts) ->
    ranch_tcp_uring:listen(normalize_trans_opts(TransOpts)).

accept(ListenRef, Timeout) ->
    ranch_tcp_uring:accept(ListenRef, Timeout).

handshake(Socket, Opts) ->
    ranch_tcp_uring:handshake(Socket, Opts).

handshake(Socket, Opts, Timeout) ->
    ranch_tcp_uring:handshake(Socket, Opts, Timeout).

handshake_continue(Socket, Opts) ->
    _ = Opts,
    {ok, Socket}.

handshake_continue(Socket, Opts, Timeout) ->
    _ = Opts,
    _ = Timeout,
    {ok, Socket}.

handshake_cancel(Socket) ->
    _ = Socket,
    ok.

recv(Socket, Length, Timeout) ->
    ranch_tcp_uring:recv(Socket, Length, Timeout).

send(Socket, Data) ->
    ranch_tcp_uring:send(Socket, Data).

sendv(Socket, Packets) ->
    ranch_tcp_uring:sendv(Socket, Packets).

batch(Socket, Commands) ->
    ranch_tcp_uring:batch(Socket, Commands).

batch(Socket, Commands, Timeout) ->
    ranch_tcp_uring:batch(Socket, Commands, Timeout).

sendfile(Socket, Filename) ->
    ranch_tcp_uring:sendfile(Socket, Filename).

sendfile(Socket, File, Offset, Bytes) ->
    ranch_tcp_uring:sendfile(Socket, File, Offset, Bytes).

sendfile(Socket, File, Offset, Bytes, Opts) ->
    ranch_tcp_uring:sendfile(Socket, File, Offset, Bytes, Opts).

setopts(Socket, Opts) ->
    ranch_tcp_uring:setopts(Socket, Opts).

getopts(Socket, Opts) ->
    ranch_tcp_uring:getopts(Socket, Opts).

getstat(Socket) ->
    ranch_tcp_uring:getstat(Socket).

getstat(Socket, OptionNames) ->
    ranch_tcp_uring:getstat(Socket, OptionNames).

controlling_process(Socket, Pid) ->
    ranch_tcp_uring:controlling_process(Socket, Pid).

peername(Socket) ->
    ranch_tcp_uring:peername(Socket).

sockname(Socket) ->
    ranch_tcp_uring:sockname(Socket).

shutdown(Socket, How) ->
    ranch_tcp_uring:shutdown(Socket, How).

close(Socket) ->
    ranch_tcp_uring:close(Socket).

connect(Host, Port, Opts) ->
    ranch_tcp_uring:connect(Host, Port, Opts).

connect(Host, Port, Opts, Timeout) ->
    ranch_tcp_uring:connect(Host, Port, Opts, Timeout).

recv_proxy_header(Socket, Timeout) ->
    ranch_tcp_uring:recv_proxy_header(Socket, Timeout).

cleanup(TransOpts) ->
    ranch_tcp_uring:cleanup(normalize_trans_opts(TransOpts)).

normalize_trans_opts(TransOpts) when is_map(TransOpts) ->
    TransOpts;
normalize_trans_opts(TransOpts) when is_list(TransOpts) ->
    #{socket_opts => TransOpts}.
