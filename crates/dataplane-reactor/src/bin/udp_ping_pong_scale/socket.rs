use std::net::{SocketAddr, UdpSocket};
use std::os::fd::{AsRawFd, RawFd};
use std::thread;

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

pub(crate) fn prepare_shared_server(server_sockets: usize) -> (SocketAddr, Vec<UdpSocket>) {
    let sockets = bind_shared_server_sockets(server_sockets);
    let addr = sockets[0].local_addr().expect("shared server addr");
    (addr, sockets)
}

pub(crate) fn bind_shared_server_sockets(count: usize) -> Vec<UdpSocket> {
    let count = count.max(1);
    let any_addr: SocketAddr = "127.0.0.1:0".parse().expect("parse any addr");
    let first = bind_udp_socket(any_addr, count > 1);
    let shared_addr = first.local_addr().expect("first socket local addr");

    let mut out = Vec::with_capacity(count);
    out.push(first);
    for _ in 1..count {
        out.push(bind_udp_socket(shared_addr, true));
    }
    out
}

pub(crate) fn bind_udp_socket(addr: SocketAddr, reuse_port: bool) -> UdpSocket {
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP)).expect("create udp socket");
    socket
        .set_reuse_address(true)
        .expect("set SO_REUSEADDR on server socket");
    if reuse_port {
        set_socket_reuse_port(&socket);
    }
    socket
        .bind(&SockAddr::from(addr))
        .expect("bind server socket");
    socket
        .set_nonblocking(true)
        .expect("set nonblocking server socket");
    socket.into()
}

pub(crate) fn set_socket_reuse_port(socket: &Socket) {
    let fd = socket.as_raw_fd();
    let yes: libc::c_int = 1;
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            (&yes as *const libc::c_int).cast(),
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    assert_eq!(
        rc,
        0,
        "setsockopt(SO_REUSEPORT) failed: {}",
        std::io::Error::last_os_error()
    );
}

pub(crate) fn connect_client_socket(server_addr: SocketAddr, nonblocking: bool) -> UdpSocket {
    let client = UdpSocket::bind("127.0.0.1:0").expect("bind udp client");
    client
        .set_nonblocking(nonblocking)
        .expect("set nonblocking client");
    client
        .connect(server_addr)
        .expect("connect client to shared server");
    client
}

pub(crate) fn pin_current_thread(shard: usize) {
    let cpu_count = thread::available_parallelism()
        .map(|c| c.get())
        .unwrap_or(1);
    let cpu = shard % cpu_count;
    let mut set = unsafe { std::mem::zeroed::<libc::cpu_set_t>() };
    unsafe {
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(cpu, &mut set);
        let rc = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
        assert_eq!(rc, 0, "sched_setaffinity failed");
    }
}

pub(crate) fn spawn_host_thread<F, T>(
    name: String,
    pin_shard: Option<usize>,
    f: F,
) -> thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    thread::Builder::new()
        .name(name.clone())
        .spawn(move || {
            if let Some(shard) = pin_shard {
                pin_current_thread(shard);
            }
            f()
        })
        .unwrap_or_else(|err| panic!("spawn host thread {name} failed: {err}"))
}

pub(crate) fn last_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

pub(crate) fn is_would_block(errno: i32) -> bool {
    errno == libc::EAGAIN || errno == libc::EWOULDBLOCK
}

pub(crate) fn sendto_nb_retry(
    fd: RawFd,
    ptr: *const u8,
    len: usize,
    addr: *const libc::sockaddr,
    addr_len: libc::socklen_t,
) {
    loop {
        let sent = unsafe { libc::sendto(fd, ptr.cast(), len, libc::MSG_DONTWAIT, addr, addr_len) };
        if sent >= 0 {
            assert_eq!(sent as usize, len, "shared server sent partial datagram");
            return;
        }
        let errno = last_errno();
        if errno == libc::EINTR || is_would_block(errno) {
            std::hint::spin_loop();
            continue;
        }
        panic!("sendto failed with errno={errno}");
    }
}
