-module(ranch_uring_SUITE).
-behaviour(ranch_protocol).

-include_lib("common_test/include/ct.hrl").
-include_lib("stdlib/include/assert.hrl").

-export([
    all/0,
    groups/0,
    init_per_suite/1,
    end_per_suite/1,
    init_per_group/2,
    end_per_group/2,
    init_per_testcase/2,
    end_per_testcase/2
]).

-export([
    listen_specified_port/1,
    listen_random_port/1,
    runtime_restart_after_stop/1,
    accept_connected_client/1,
    accept_timeout/1,
    accept_timeout_cleanup/1,
    concurrent_acceptors/1,
    send_recv_roundtrip/1,
    nif_resource_registration_recv_recovery/1,
    subscribe_command_read_control/1,
    subscribe_command_accept/1,
    subscribe_command_accept_new_consumer/1,
    demux_future_stat/1,
    demux_accept_recv_roundtrip/1,
    demux_recv_timeout/1,
    recv_timeout/1,
    recv_timeout_recovery/1,
    large_transfer/1,
    sendfile_transfer/1,
    setopts_getopts/1,
    passive_mode_recv/1,
    active_mode_delivery/1,
    active_once/1,
    active_n/1,
    controlling_process_transfer/1,
    controlling_process_death/1,
    stale_owner_watch_does_not_close_replaced_owner/1,
    owner_death_closes_client/1,
    shutdown_write/1,
    shutdown_read_write/1,
    close_async_repeated_reply/1,
    close_no_leak/1,
    http11_request_response/1,
    http11_keepalive/1,
    concurrent_connections/1,
    sequential_cycle/1,
    abrupt_client_disconnect/1,
    server_close_during_send/1
]).

-export([echo_loop/3]).
-export([start_link/3]).

-define(LISTENER, ranch_uring_test_listener).
-define(ECHO_PROTO, ?MODULE).

all() ->
    [
        {group, listener_lifecycle},
        {group, data_transfer},
        {group, socket_options},
        {group, process_ownership},
        {group, shutdown_close},
        {group, cowboy_smoke},
        {group, stress}
    ].

groups() ->
    [
        {listener_lifecycle, [sequence], [
            listen_specified_port,
            listen_random_port,
            runtime_restart_after_stop,
            accept_connected_client,
            accept_timeout,
            accept_timeout_cleanup,
            concurrent_acceptors
        ]},
        {data_transfer, [sequence], [
            send_recv_roundtrip,
            nif_resource_registration_recv_recovery,
            subscribe_command_read_control,
            subscribe_command_accept,
            subscribe_command_accept_new_consumer,
            demux_future_stat,
            demux_accept_recv_roundtrip,
            demux_recv_timeout,
            recv_timeout,
            recv_timeout_recovery,
            large_transfer,
            sendfile_transfer
        ]},
        {socket_options, [sequence], [
            setopts_getopts,
            passive_mode_recv,
            active_mode_delivery,
            active_once,
            active_n
        ]},
        {process_ownership, [sequence], [
            controlling_process_transfer,
            controlling_process_death,
            stale_owner_watch_does_not_close_replaced_owner,
            owner_death_closes_client
        ]},
        {shutdown_close, [sequence], [
            shutdown_write,
            shutdown_read_write,
            close_async_repeated_reply,
            close_no_leak
        ]},
        {cowboy_smoke, [sequence], [
            http11_request_response,
            http11_keepalive,
            concurrent_connections
        ]},
        {stress, [sequence], [
            sequential_cycle,
            abrupt_client_disconnect,
            server_close_during_send
        ]}
    ].

init_per_suite(Config) ->
    {ok, _} = application:ensure_all_started(ranch),
    {ok, _} = application:ensure_all_started(ranch_uring),
    Config.

end_per_suite(_Config) ->
    ok.

init_per_group(socket_options, Config) -> Config;
init_per_group(process_ownership, Config) -> Config;
init_per_group(cowboy_smoke, _Config) -> {skip, cowboy_not_available};
init_per_group(stress, _Config) -> {skip, stress_tests_pending};
init_per_group(_, Config) -> Config.

end_per_group(_Group, _Config) ->
    ok.

init_per_testcase(TestCase, Config) ->
    ListenerName = list_to_atom(atom_to_list(TestCase) ++ "_listener"),
    {ok, _} = ranch:start_listener(
        ListenerName,
        ranch_uring,
        #{socket_opts => [{port, 0}]},
        ?ECHO_PROTO,
        #{}
    ),
    Port = ranch:get_port(ListenerName),
    [{listener, ListenerName}, {port, Port} | Config].

end_per_testcase(_TestCase, Config) ->
    ListenerName = proplists:get_value(listener, Config),
    catch ranch:stop_listener(ListenerName),
    catch ranch_uring_demux:stop_pool(),
    ok.

%% ============================================================================
%% Group: listener_lifecycle
%% ============================================================================

listen_specified_port(Config) ->
    Port = proplists:get_value(port, Config),
    ?assert(Port > 0),
    {ok, Sock} = ranch_uring:listen([{port, Port + 1000}]),
    {ok, {_Addr, AssignedPort}} = ranch_uring:sockname(Sock),
    ?assertEqual(Port + 1000, AssignedPort),
    ranch_uring:close(Sock).

listen_random_port(_Config) ->
    {ok, Sock} = ranch_uring:listen([{port, 0}]),
    {ok, {_Addr, Port}} = ranch_uring:sockname(Sock),
    ?assert(Port > 0),
    ranch_uring:close(Sock).

runtime_restart_after_stop(_Config) ->
    ok = application:stop(ranch_uring),
    ok = application:stop(ranch),
    {ok, _} = application:ensure_all_started(ranch_uring),
    ok.

accept_connected_client(Config) ->
    _Port = proplists:get_value(port, Config),
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, _ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock).

accept_timeout(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    Result = ranch_uring:accept(ListenSock, 500),
    ?assertEqual({error, timeout}, Result),
    ranch_uring:close(ListenSock).

accept_timeout_cleanup(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    ?assertEqual({error, timeout}, ranch_uring:accept(ListenSock, 100)),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

concurrent_acceptors(Config) ->
    _Port = proplists:get_value(port, Config),
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    Self = self(),
    NumClients = 5,
    AcceptorFun = fun() ->
        case ranch_uring:accept(ListenSock, 3000) of
            {ok, Sock} ->
                Self ! {accepted, Sock};
            {error, Reason} ->
                Self ! {accept_error, Reason}
        end
    end,
    [spawn(AcceptorFun) || _ <- lists:seq(1, NumClients)],
    [gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]) || _ <- lists:seq(1, NumClients)],
    Results = [receive Msg -> Msg after 5000 -> timeout end || _ <- lists:seq(1, NumClients)],
    Accepted = [Sock || {accepted, Sock} <- Results],
    ?assertEqual(NumClients, length(Accepted)),
    [ranch_uring:close(S) || S <- Accepted],
    ranch_uring:close(ListenSock).

%% ============================================================================
%% Group: data_transfer
%% ============================================================================

nif_resource_registration_recv_recovery(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, Port}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", Port, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    %% Direct NIF calls exercise Rustler resource registration and lifetime while
    %% a timed-out receive is followed by a successful receive on the same handle.
    ?assertEqual({error, timeout}, ranch_uring_nif:recv(ServerSock, 0, 100)),
    Payload = <<"nif-resource-recovery">>,
    ok = gen_tcp:send(ClientSock, Payload),
    ?assertEqual({ok, Payload}, ranch_uring_nif:recv(ServerSock, byte_size(Payload), 2000)),
    ok = ranch_uring_nif:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).


send_recv_roundtrip(Config) ->
    Port = proplists:get_value(port, Config),
    {ok, ClientSock} = connect_client(Port),
    Payload = <<"hello ranch_uring">>,
    ok = gen_tcp:send(ClientSock, Payload),
    {ok, Received} = gen_tcp:recv(ClientSock, byte_size(Payload), 3000),
    ?assertEqual(Payload, Received),
    gen_tcp:close(ClientSock).

subscribe_command_read_control(Config) ->
    Port = proplists:get_value(port, Config),
    {ok, ClientSock} = connect_client(Port),
    {ok, Fd} = prim_inet:getfd(ClientSock),
    {ok, SubscriptionId} = ranch_uring_nif:command({subscribe, self(), read, Fd}),
    Payload1 = <<"subscribe-payload">>,
    ok = gen_tcp:send(ClientSock, Payload1),
    {ok, Payload1} = await_subscribe_payload(SubscriptionId, 2000),
    ok = ranch_uring_nif:command({choke, SubscriptionId}),
    ok = ranch_uring_nif:command({unchoke, SubscriptionId}),
    ok = ranch_uring_nif:command({stop, SubscriptionId}),
    Payload2 = <<"after-stop">>,
    ok = gen_tcp:send(ClientSock, Payload2),
    ?assertEqual(timeout, await_subscribe_payload(SubscriptionId, 300)),
    gen_tcp:close(ClientSock).

subscribe_command_accept(_Config) ->
    {ok, ListenSock} = gen_tcp:listen(0, [binary, {active, false}, {packet, raw}, {reuseaddr, true}]),
    {ok, {_, Port}} = inet:sockname(ListenSock),
    {ok, Fd} = prim_inet:getfd(ListenSock),
    {ok, SubscriptionId} = ranch_uring_nif:command({subscribe, self(), accept, Fd}),
    {ok, ClientSock} = gen_tcp:connect("localhost", Port, [binary, {active, false}, {packet, raw}]),
    {ok, ServerSock} = await_subscribe_session(SubscriptionId, 2000),
    Payload = <<"subscribe-accept">>,
    ok = gen_tcp:send(ClientSock, Payload),
    {ok, Payload} = ranch_uring_nif:recv(ServerSock, byte_size(Payload), 2000),
    ok = ranch_uring_nif:close(ServerSock),
    ok = ranch_uring_nif:command({stop, SubscriptionId}),
    gen_tcp:close(ClientSock),
    gen_tcp:close(ListenSock).

subscribe_command_accept_new_consumer(_Config) ->
    {ok, ListenSock} = gen_tcp:listen(0, [binary, {active, false}, {packet, raw}, {reuseaddr, true}]),
    {ok, {_, Port}} = inet:sockname(ListenSock),
    {ok, Fd} = prim_inet:getfd(ListenSock),
    {ok, SubscriptionId} = ranch_uring_nif:command({subscribe, self(), accept, Fd}),
    Parent = self(),
    ConsumerPid = spawn_link(fun() ->
        Parent ! {consumer_registered, self(), ranch_uring_nif:command({subscribe, new_consumer, SubscriptionId})},
        await_subscribe_session_as_consumer(Parent, SubscriptionId)
    end),
    receive
        {consumer_registered, ConsumerPid, ok} ->
            ok
    after 2000 ->
        ?assert(false)
    end,
    timer:sleep(20),
    Clients = [begin
        {ok, C} = gen_tcp:connect("localhost", Port, [binary, {active, false}, {packet, raw}]),
        C
    end || _ <- lists:seq(1, 12)],
    {true, true} = await_dual_consumer_hits(SubscriptionId, ConsumerPid, false, false, 3000),
    ok = ranch_uring_nif:command({stop, SubscriptionId}),
    lists:foreach(fun gen_tcp:close/1, Clients),
    gen_tcp:close(ListenSock).

demux_future_stat(_Config) ->
    {ok, _Demuxes} = ranch_uring_demux:start_pool(4),
    {ok, Future = {_DemuxPid, _ReqId}} = ranch_uring_demux:submit_stat(0),
    {ok, Size} = ranch_uring_demux:await(Future, 2000),
    ?assert(is_integer(Size)),
    ?assert(Size >= 0),
    ok.

demux_accept_recv_roundtrip(_Config) ->
    {ok, _Demuxes} = ranch_uring_demux:start_pool(4),
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:setopts(ServerSock, [{active, false}]),
    Payload = <<"demux_roundtrip_payload">>,
    ok = gen_tcp:send(ClientSock, Payload),
    {ok, Payload} = ranch_uring:recv(ServerSock, byte_size(Payload), 2000),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

demux_recv_timeout(_Config) ->
    {ok, _Demuxes} = ranch_uring_demux:start_pool(2),
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:setopts(ServerSock, [{active, false}]),
    ?assertEqual({error, timeout}, ranch_uring:recv(ServerSock, 0, 200)),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

recv_timeout(Config) ->
    _Port = proplists:get_value(port, Config),
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, _ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:setopts(ServerSock, [{active, false}]),
    Result = ranch_uring:recv(ServerSock, 0, 500),
    ?assertEqual({error, timeout}, Result),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock).

recv_timeout_recovery(Config) ->
    _Port = proplists:get_value(port, Config),
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:setopts(ServerSock, [{active, false}]),
    ?assertEqual({error, timeout}, ranch_uring:recv(ServerSock, 0, 50)),
    Payload = <<"timeout-recovery">>,
    ok = gen_tcp:send(ClientSock, Payload),
    {ok, Payload} = ranch_uring:recv(ServerSock, byte_size(Payload), 2000),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

large_transfer(Config) ->
    Port = proplists:get_value(port, Config),
    {ok, ClientSock} = connect_client(Port),
    DataSize = 128 * 1024,
    Payload = crypto:strong_rand_bytes(DataSize),
    ok = gen_tcp:send(ClientSock, Payload),
    {ok, Received} = recv_all(ClientSock, DataSize, 10000),
    ?assertEqual(crypto:hash(sha256, Payload), crypto:hash(sha256, Received)),
    gen_tcp:close(ClientSock).

sendfile_transfer(Config) ->
    Port = proplists:get_value(port, Config),
    {ok, ClientSock} = connect_client(Port),
    TmpFile = filename:join(proplists:get_value(priv_dir, Config, "/tmp"), "ranch_uring_sendfile_test"),
    FileData = crypto:strong_rand_bytes(32768),
    ok = file:write_file(TmpFile, FileData),
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, SendClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    {ok, Fd} = file:open(TmpFile, [read, raw, binary]),
    {ok, _Sent} = ranch_uring:sendfile(ServerSock, Fd, 0, byte_size(FileData), []),
    file:close(Fd),
    {ok, Received} = recv_all(SendClientSock, byte_size(FileData), 10000),
    ?assertEqual(FileData, Received),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(SendClientSock),
    gen_tcp:close(ClientSock),
    file:delete(TmpFile).

%% ============================================================================
%% Group: socket_options
%% ============================================================================

setopts_getopts(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, _ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:setopts(ServerSock, [{active, false}, {packet, raw}, {buffer, 8192}, {nodelay, true}]),
    {ok, Opts} = ranch_uring:getopts(ServerSock, [active, packet, buffer, nodelay]),
    ?assertEqual(false, proplists:get_value(active, Opts)),
    ?assertEqual(raw, proplists:get_value(packet, Opts)),
    ?assert(proplists:get_value(buffer, Opts) >= 8192),
    ?assertEqual(true, proplists:get_value(nodelay, Opts)),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock).

passive_mode_recv(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:setopts(ServerSock, [{active, false}]),
    Payload = <<"passive_test">>,
    ok = gen_tcp:send(ClientSock, Payload),
    {ok, Data} = ranch_uring:recv(ServerSock, byte_size(Payload), 3000),
    ?assertEqual(Payload, Data),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

active_mode_delivery(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:setopts(ServerSock, [{active, true}]),
    Payload = <<"active_delivery_test">>,
    ok = gen_tcp:send(ClientSock, Payload),
    receive
        {uring_tcp, ServerSock, Data} ->
            ?assertEqual(Payload, Data)
    after 3000 ->
        ct:fail(no_active_message)
    end,
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

active_once(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:setopts(ServerSock, [{active, once}]),
    Payload1 = <<"first">>,
    Payload2 = <<"second">>,
    ok = gen_tcp:send(ClientSock, Payload1),
    receive
        {uring_tcp, ServerSock, Payload1} -> ok
    after 3000 ->
        ct:fail(no_first_message)
    end,
    ok = gen_tcp:send(ClientSock, Payload2),
    receive
        {uring_tcp, ServerSock, _} ->
            ct:fail(unexpected_second_message)
    after 500 ->
        ok
    end,
    ok = ranch_uring:setopts(ServerSock, [{active, once}]),
    receive
        {uring_tcp, ServerSock, Payload2} -> ok
    after 3000 ->
        ct:fail(no_second_message_after_setopts)
    end,
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

active_n(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:setopts(ServerSock, [{active, 3}]),
    ok = gen_tcp:send(ClientSock, <<"msg1">>),
    Msgs1 = collect_active_messages(ServerSock, 1, 5000),
    ?assertEqual([<<"msg1">>], Msgs1),
    ok = gen_tcp:send(ClientSock, <<"msg2">>),
    Msgs2 = collect_active_messages(ServerSock, 1, 5000),
    ?assertEqual([<<"msg2">>], Msgs2),
    ok = gen_tcp:send(ClientSock, <<"msg3">>),
    Msgs3 = collect_active_messages(ServerSock, 1, 5000),
    ?assertEqual([<<"msg3">>], Msgs3),
    receive
        {uring_tcp_passive, ServerSock} -> ok
    after 3000 ->
        ct:fail(no_passive_notification)
    end,
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

%% ============================================================================
%% Group: process_ownership
%% ============================================================================

controlling_process_transfer(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    Self = self(),
    NewOwner = spawn(fun() ->
        receive
            {take_socket, Sock} ->
                ranch_uring:setopts(Sock, [{active, true}]),
                receive
                    {uring_tcp, Sock, Data} ->
                        Self ! {received, Data}
                after 3000 ->
                    Self ! owner_timeout
                end
        end
    end),
    ok = ranch_uring:controlling_process(ServerSock, NewOwner),
    NewOwner ! {take_socket, ServerSock},
    Payload = <<"ownership_test">>,
    ok = gen_tcp:send(ClientSock, Payload),
    receive
        {received, Payload} -> ok
    after 3000 ->
        ct:fail(transfer_failed)
    end,
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

controlling_process_death(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, _ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    NewOwner = spawn(fun() -> receive die -> ok end end),
    ok = ranch_uring:controlling_process(ServerSock, NewOwner),
    MonRef = monitor(process, NewOwner),
    NewOwner ! die,
    receive
        {'DOWN', MonRef, process, NewOwner, _} -> ok
    after 3000 ->
        ct:fail(owner_did_not_die)
    end,
    timer:sleep(100),
    Result = ranch_uring:send(ServerSock, <<"probe">>),
    ?assertMatch({error, _}, Result),
    ranch_uring:close(ListenSock).

stale_owner_watch_does_not_close_replaced_owner(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    Owner1 = spawn(fun() -> receive die -> ok end end),
    Owner2 = spawn(fun() -> receive die -> ok end end),
    ok = ranch_uring:controlling_process(ServerSock, Owner1),
    ok = ranch_uring:controlling_process(ServerSock, Owner2),
    MonRef1 = monitor(process, Owner1),
    Owner1 ! die,
    receive
        {'DOWN', MonRef1, process, Owner1, _} -> ok
    after 3000 ->
        ct:fail(first_owner_did_not_die)
    end,
    timer:sleep(100),
    ok = gen_tcp:send(ClientSock, <<"still-open">>),
    ok = ranch_uring:setopts(ServerSock, [{active, false}]),
    ?assertEqual({ok, <<"still-open">>}, ranch_uring:recv(ServerSock, 10, 3000)),
    ok = ranch_uring:send(ServerSock, <<"server-open">>),
    ?assertEqual({ok, <<"server-open">>}, gen_tcp:recv(ClientSock, 11, 3000)),
    MonRef2 = monitor(process, Owner2),
    Owner2 ! die,
    receive
        {'DOWN', MonRef2, process, Owner2, _} -> ok
    after 3000 ->
        ct:fail(second_owner_did_not_die)
    end,
    ?assertMatch({error, closed}, gen_tcp:recv(ClientSock, 0, 3000)),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

owner_death_closes_client(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    NewOwner = spawn(fun() -> receive die -> ok end end),
    ok = ranch_uring:controlling_process(ServerSock, NewOwner),
    NewOwner ! die,
    ?assertMatch({error, closed}, gen_tcp:recv(ClientSock, 0, 3000)),
    ?assertMatch({error, _}, ranch_uring:send(ServerSock, <<"probe">>)),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

%% ============================================================================
%% Group: shutdown_close
%% ============================================================================

shutdown_write(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:shutdown(ServerSock, write),
    Result = gen_tcp:recv(ClientSock, 0, 3000),
    ?assertMatch({error, closed}, Result),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

shutdown_read_write(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ok = ranch_uring:shutdown(ServerSock, read_write),
    Result = gen_tcp:recv(ClientSock, 0, 3000),
    ?assertMatch({error, closed}, Result),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

close_async_repeated_reply(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    {ok, FirstRequestId} = ranch_uring_nif:close_async(ServerSock),
    {ok, SecondRequestId} = ranch_uring_nif:close_async(ServerSock),
    ok = receive_async_ok(FirstRequestId, 2000),
    ok = receive_async_ok(SecondRequestId, 2000),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

close_no_leak(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    InitialProcs = erlang:system_info(process_count),
    Cycles = 1000,
    lists:foreach(fun(_) ->
        {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
        {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
        ranch_uring:close(ServerSock),
        gen_tcp:close(ClientSock)
    end, lists:seq(1, Cycles)),
    FinalProcs = erlang:system_info(process_count),
    ct:pal("Process count: before=~p after=~p delta=~p", [InitialProcs, FinalProcs, FinalProcs - InitialProcs]),
    ?assert(FinalProcs - InitialProcs < 50),
    ranch_uring:close(ListenSock).

%% ============================================================================
%% Group: cowboy_smoke
%% ============================================================================

http11_request_response(Config) ->
    Port = proplists:get_value(port, Config),
    {ok, ConnSock} = gen_tcp:connect("localhost", Port, [binary, {active, false}, {packet, http}]),
    Request = <<"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n">>,
    ok = gen_tcp:send(ConnSock, Request),
    {ok, {http_response, {1, 1}, 200, _}} = gen_tcp:recv(ConnSock, 0, 5000),
    gen_tcp:close(ConnSock).

http11_keepalive(Config) ->
    Port = proplists:get_value(port, Config),
    {ok, ConnSock} = gen_tcp:connect("localhost", Port, [binary, {active, false}, {packet, http}]),
    lists:foreach(fun(_) ->
        Request = <<"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n">>,
        ok = gen_tcp:send(ConnSock, Request),
        {ok, {http_response, {1, 1}, 200, _}} = gen_tcp:recv(ConnSock, 0, 5000),
        drain_http_response(ConnSock)
    end, lists:seq(1, 3)),
    gen_tcp:close(ConnSock).

concurrent_connections(Config) ->
    Port = proplists:get_value(port, Config),
    Self = self(),
    NumConns = 100,
    [spawn(fun() ->
        Result = gen_tcp:connect("localhost", Port, [binary, {active, false}]),
        Self ! {conn_result, Result}
    end) || _ <- lists:seq(1, NumConns)],
    Results = [receive {conn_result, R} -> R after 10000 -> timeout end || _ <- lists:seq(1, NumConns)],
    Successes = [ok || {ok, _} <- Results],
    ct:pal("Concurrent connections: ~p/~p succeeded", [length(Successes), NumConns]),
    ?assertEqual(NumConns, length(Successes)),
    [gen_tcp:close(S) || {ok, S} <- Results].

%% ============================================================================
%% Group: stress
%% ============================================================================

sequential_cycle(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    Cycles = 1000,
    lists:foreach(fun(I) ->
        {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
        {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
        Payload = integer_to_binary(I),
        ok = ranch_uring:send(ServerSock, Payload),
        {ok, Payload} = gen_tcp:recv(ClientSock, byte_size(Payload), 3000),
        ranch_uring:close(ServerSock),
        gen_tcp:close(ClientSock)
    end, lists:seq(1, Cycles)),
    ranch_uring:close(ListenSock).

abrupt_client_disconnect(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    gen_tcp:close(ClientSock),
    timer:sleep(50),
    Result = ranch_uring:recv(ServerSock, 0, 500),
    ?assertMatch({error, _}, Result),
    ranch_uring:close(ServerSock),
    ranch_uring:close(ListenSock).

server_close_during_send(_Config) ->
    {ok, ListenSock} = ranch_uring:listen([{port, 0}]),
    {ok, {_, ListenPort}} = ranch_uring:sockname(ListenSock),
    {ok, ClientSock} = gen_tcp:connect("localhost", ListenPort, [binary, {active, false}]),
    {ok, ServerSock} = ranch_uring:accept(ListenSock, 2000),
    ranch_uring:close(ServerSock),
    Result = gen_tcp:send(ClientSock, crypto:strong_rand_bytes(64 * 1024)),
    ?assertMatch({error, _}, Result),
    ranch_uring:close(ListenSock),
    gen_tcp:close(ClientSock).

%% ============================================================================
%% Echo protocol handler (Ranch protocol behaviour)
%% ============================================================================

start_link(Ref, Transport, Opts) ->
    SpawnOpts = case mqd_opt(Opts) of
        off_heap -> [{message_queue_data, off_heap}];
        on_heap -> [{message_queue_data, on_heap}];
        _ -> []
    end,
    Pid = spawn_opt(?MODULE, echo_loop, [Ref, Transport, Opts], [link | SpawnOpts]),
    {ok, Pid}.

mqd_opt(Opts) when is_map(Opts) ->
    maps:get(message_queue_data, Opts, off_heap);
mqd_opt(Opts) ->
    proplists:get_value(message_queue_data, Opts, off_heap).

echo_loop(Ref, Transport, _Opts) ->
    {ok, Socket} = ranch:handshake(Ref),
    ok = Transport:setopts(Socket, [{active, false}]),
    do_echo(Socket, Transport).

do_echo(Socket, Transport) ->
    case Transport:recv(Socket, 0, 5000) of
        {ok, Data} ->
            ok = Transport:send(Socket, Data),
            do_echo(Socket, Transport);
        {error, _} ->
            Transport:close(Socket)
    end.

%% ============================================================================
%% Helpers
%% ============================================================================

connect_client(Port) ->
    gen_tcp:connect("localhost", Port, [binary, {active, false}]).

receive_async_ok(RequestId, Timeout) ->
    receive
        {reply, RequestId, ok} ->
            ok;
        {reply, RequestId, Other} ->
            {error, Other}
    after Timeout ->
        {error, timeout}
    end.

recv_all(Sock, TotalSize, Timeout) ->
    Deadline = erlang:monotonic_time(millisecond) + Timeout,
    recv_all_loop(Sock, TotalSize, Deadline, []).

recv_all_loop(_Sock, 0, _Deadline, Acc) ->
    {ok, iolist_to_binary(lists:reverse(Acc))};
recv_all_loop(Sock, Remaining, Deadline, Acc) ->
    Now = erlang:monotonic_time(millisecond),
    TimeLeft = Deadline - Now,
    case TimeLeft =< 0 of
        true -> {error, timeout};
        false ->
            case gen_tcp:recv(Sock, min(Remaining, 65536), TimeLeft) of
                {ok, Data} ->
                    recv_all_loop(Sock, Remaining - byte_size(Data), Deadline, [Data | Acc]);
                {error, Reason} ->
                    {error, Reason}
            end
    end.

collect_active_messages(Sock, N, Timeout) ->
    collect_active_messages(Sock, N, Timeout, []).

collect_active_messages(_Sock, 0, _Timeout, Acc) ->
    lists:reverse(Acc);
collect_active_messages(Sock, N, Timeout, Acc) ->
    receive
        {uring_tcp, Sock, Data} ->
            collect_active_messages(Sock, N - 1, Timeout, [Data | Acc])
    after Timeout ->
        lists:reverse(Acc)
    end.

drain_http_response(Sock) ->
    case gen_tcp:recv(Sock, 0, 2000) of
        {ok, http_eoh} -> ok;
        {ok, _} -> drain_http_response(Sock);
        {error, _} -> ok
    end.

await_subscribe_payload(SubscriptionId, TimeoutMs) ->
    Deadline = erlang:monotonic_time(millisecond) + TimeoutMs,
    await_subscribe_payload_deadline(SubscriptionId, Deadline).

await_subscribe_payload_deadline(SubscriptionId, Deadline) ->
    Remaining = Deadline - erlang:monotonic_time(millisecond),
    case Remaining =< 0 of
        true ->
            timeout;
        false ->
            receive
                {reply, SubscriptionId, {ok, Data}} when is_binary(Data) ->
                    {ok, Data};
                {reply, SubscriptionId, {error, Reason}} ->
                    {error, Reason};
                {nif_results, Pairs} ->
                    case lists:keyfind(SubscriptionId, 1, Pairs) of
                        {SubscriptionId, {ok, Data}} when is_binary(Data) ->
                            {ok, Data};
                        {SubscriptionId, {error, Reason}} ->
                            {error, Reason};
                        false ->
                            await_subscribe_payload_deadline(SubscriptionId, Deadline)
                    end
            after Remaining ->
                timeout
            end
    end.

await_subscribe_session(SubscriptionId, TimeoutMs) ->
    Deadline = erlang:monotonic_time(millisecond) + TimeoutMs,
    await_subscribe_session_deadline(SubscriptionId, Deadline).

await_subscribe_session_deadline(SubscriptionId, Deadline) ->
    Remaining = Deadline - erlang:monotonic_time(millisecond),
    case Remaining =< 0 of
        true ->
            timeout;
        false ->
            receive
                {reply, SubscriptionId, {ok, Session}} ->
                    {ok, Session};
                {reply, SubscriptionId, {error, Reason}} ->
                    {error, Reason};
                {nif_results, Pairs} ->
                    case lists:keyfind(SubscriptionId, 1, Pairs) of
                        {SubscriptionId, {ok, Session}} ->
                            {ok, Session};
                        {SubscriptionId, {error, Reason}} ->
                            {error, Reason};
                        false ->
                            await_subscribe_session_deadline(SubscriptionId, Deadline)
                    end
            after Remaining ->
                timeout
            end
    end.

await_subscribe_session_as_consumer(Parent, SubscriptionId) ->
    receive
        {reply, SubscriptionId, {ok, Session}} ->
            ok = ranch_uring_nif:close(Session),
            Parent ! {consumer_hit, self()},
            ok;
        {nif_results, Pairs} ->
            case lists:keyfind(SubscriptionId, 1, Pairs) of
                {SubscriptionId, {ok, Session}} ->
                    ok = ranch_uring_nif:close(Session),
                    Parent ! {consumer_hit, self()},
                    ok;
                _ ->
                    await_subscribe_session_as_consumer(Parent, SubscriptionId)
            end;
        _Other ->
            await_subscribe_session_as_consumer(Parent, SubscriptionId)
    after 3000 ->
        Parent ! {consumer_hit_timeout, self()},
        ok
    end.

await_dual_consumer_hits(_SubscriptionId, _ConsumerPid, SeenSelf, SeenConsumer, _TimeoutMs)
    when SeenSelf =:= true, SeenConsumer =:= true ->
    {true, true};
await_dual_consumer_hits(SubscriptionId, ConsumerPid, SeenSelf, SeenConsumer, TimeoutMs) ->
    receive
        {reply, SubscriptionId, {ok, Session}} ->
            ok = ranch_uring_nif:close(Session),
            await_dual_consumer_hits(SubscriptionId, ConsumerPid, true, SeenConsumer, TimeoutMs);
        {nif_results, Pairs} ->
            case lists:keyfind(SubscriptionId, 1, Pairs) of
                {SubscriptionId, {ok, Session}} ->
                    ok = ranch_uring_nif:close(Session),
                    await_dual_consumer_hits(SubscriptionId, ConsumerPid, true, SeenConsumer, TimeoutMs);
                _ ->
                    await_dual_consumer_hits(SubscriptionId, ConsumerPid, SeenSelf, SeenConsumer, TimeoutMs)
            end;
        {consumer_hit, ConsumerPid} ->
            await_dual_consumer_hits(SubscriptionId, ConsumerPid, SeenSelf, true, TimeoutMs);
        {consumer_hit_timeout, ConsumerPid} ->
            {SeenSelf, SeenConsumer}
    after TimeoutMs ->
        {SeenSelf, SeenConsumer}
    end.
