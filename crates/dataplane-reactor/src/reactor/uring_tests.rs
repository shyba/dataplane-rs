use super::UringReactor;
use crate::reactor::{NetEvent, NetOp, NetOpKind, Reactor, UdpRecvSlot};
use std::io::Read;
use std::net::UdpSocket;
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

fn close_fd(fd: RawFd) {
    let _ = unsafe { libc::close(fd) };
}

#[test]
fn send_all_completes_full_payload() {
    let mut fds = [0i32; 2];
    let rc = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
    assert_eq!(rc, 0, "socketpair failed");
    let tx_fd = fds[0];
    let rx_fd = fds[1];

    let payload = vec![0xA5u8; 256 * 1024];
    let expected_len = payload.len();

    let reader = thread::spawn(move || {
        let mut stream = unsafe { UnixStream::from_raw_fd(rx_fd) };
        let mut got = vec![0u8; expected_len];
        let mut off = 0usize;
        while off < got.len() {
            let n = stream.read(&mut got[off..]).expect("reader read");
            if n == 0 {
                break;
            }
            off += n;
        }
        got
    });

    let mut reactor = UringReactor::new(256).expect("create uring reactor");
    let token = Reactor::submit(
        &mut reactor,
        NetOp::SendAll {
            fd: tx_fd,
            ptr: payload.as_ptr(),
            len: payload.len(),
        },
    )
    .expect("submit send_all");
    reactor.submit_pending().expect("submit pending");

    let mut done = false;
    for _ in 0..2000 {
        for event in reactor.poll(true).expect("poll reactor") {
            let NetEvent::OpComplete {
                token: t,
                kind,
                result,
                ..
            } = event;
            if t == token {
                assert!(matches!(kind, NetOpKind::SendAll));
                assert_eq!(result as usize, expected_len);
                done = true;
                break;
            }
        }
        if done {
            break;
        }
    }
    assert!(done, "send_all did not complete");
    close_fd(tx_fd);

    let got = reader.join().expect("join reader");
    assert_eq!(got.len(), expected_len);
    assert!(got.iter().all(|b| *b == 0xA5));
}

#[test]
fn accept_multi_subscription_emits_event() {
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;

    struct AcceptHandler {
        tx: mpsc::Sender<Result<RawFd, i32>>,
    }

    impl crate::reactor::Handler<crate::reactor::NetSubscriptionEvent> for AcceptHandler {
        fn on_event(
            &mut self,
            _cx: &mut crate::reactor::EventContext<'_>,
            event: crate::reactor::NetSubscriptionEvent,
        ) {
            match event {
                crate::reactor::NetSubscriptionEvent::Accepted { fd } => {
                    let _ = self.tx.send(Ok(fd));
                }
            }
        }

        fn on_error(&mut self, _cx: &mut crate::reactor::EventContext<'_>, error: std::io::Error) {
            let _ = self.tx.send(Err(error.raw_os_error().unwrap_or(libc::EIO)));
        }
    }

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind listener");
    listener
        .set_nonblocking(true)
        .expect("set nonblocking listener");
    let addr = listener.local_addr().expect("listener addr");

    let mut reactor = UringReactor::new(256).expect("create uring reactor");
    let (tx, rx) = mpsc::channel();
    let _sub = reactor
        .subscribe(
            crate::reactor::NetSubscription::AcceptMulti {
                listener_fd: std::os::fd::AsRawFd::as_raw_fd(&listener),
                flags: 0,
            },
            AcceptHandler { tx },
        )
        .expect("subscribe accept multi");
    reactor.submit_pending().expect("submit pending");

    let _client = TcpStream::connect(addr).expect("connect client");
    let mut got = None;
    for _ in 0..200 {
        let _ = reactor.poll(true);
        if let Ok(msg) = rx.try_recv() {
            got = Some(msg);
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    match got {
        Some(Ok(fd)) => {
            assert!(fd >= 0);
            close_fd(fd);
        }
        Some(Err(errno)) if errno == libc::EINVAL || errno == libc::EOPNOTSUPP => {
            // Kernel without multishot accept support: treat as skipped.
        }
        Some(Err(errno)) => panic!("accept_multi error: {errno}"),
        None => panic!("no accept event received"),
    }
}

#[test]
fn udp_send_recv_completes() {
    let a = UdpSocket::bind("127.0.0.1:0").expect("bind udp a");
    let b = UdpSocket::bind("127.0.0.1:0").expect("bind udp b");
    a.set_nonblocking(true).expect("set nonblocking a");
    b.set_nonblocking(true).expect("set nonblocking b");
    let a_addr = a.local_addr().expect("a addr");
    let b_addr = b.local_addr().expect("b addr");
    a.connect(b_addr).expect("connect a->b");
    b.connect(a_addr).expect("connect b->a");

    let payload = [0x5Au8; 64];
    let mut recv_buf = [0u8; 64];

    let mut tx_reactor = UringReactor::new(64).expect("create tx reactor");
    let mut rx_reactor = UringReactor::new(64).expect("create rx reactor");

    let rx_token = Reactor::submit(
        &mut rx_reactor,
        NetOp::UdpRecv {
            fd: std::os::fd::AsRawFd::as_raw_fd(&b),
            ptr: recv_buf.as_mut_ptr(),
            len: recv_buf.len(),
        },
    )
    .expect("submit udp recv");
    rx_reactor.submit_pending().expect("submit recv pending");

    let tx_token = Reactor::submit(
        &mut tx_reactor,
        NetOp::UdpSend {
            fd: std::os::fd::AsRawFd::as_raw_fd(&a),
            ptr: payload.as_ptr(),
            len: payload.len(),
        },
    )
    .expect("submit udp send");
    tx_reactor.submit_pending().expect("submit send pending");

    let mut sent_ok = false;
    for _ in 0..200 {
        for event in tx_reactor.poll(true).expect("poll tx reactor") {
            let NetEvent::OpComplete {
                token,
                kind,
                result,
                ..
            } = event;
            if token == tx_token {
                assert_eq!(kind, NetOpKind::UdpSend);
                assert_eq!(result as usize, payload.len());
                sent_ok = true;
                break;
            }
        }
        if sent_ok {
            break;
        }
    }
    assert!(sent_ok, "udp send completion not observed");

    let mut recv_len = None;
    for _ in 0..200 {
        for event in rx_reactor.poll(true).expect("poll rx reactor") {
            let NetEvent::OpComplete {
                token,
                kind,
                result,
                ..
            } = event;
            if token == rx_token {
                assert_eq!(kind, NetOpKind::UdpRecv);
                assert!(result >= 0, "udp recv failed with errno={}", -result);
                recv_len = Some(result as usize);
                break;
            }
        }
        if recv_len.is_some() {
            break;
        }
    }
    let recv_len = recv_len.expect("udp recv completion not observed");
    assert_eq!(recv_len, payload.len());
    assert_eq!(&recv_buf[..recv_len], &payload);
}

#[test]
fn udp_recv_batch_completes_with_single_datagram() {
    let server = UdpSocket::bind("127.0.0.1:0").expect("bind server");
    server
        .set_nonblocking(true)
        .expect("set nonblocking server");
    let server_addr = server.local_addr().expect("server addr");
    let client = UdpSocket::bind("127.0.0.1:0").expect("bind client");
    let payload = b"batch";
    let sent = client.send_to(payload, server_addr).expect("send datagram");
    assert_eq!(sent, payload.len());

    let mut recv_buf = [0u8; 64];
    let mut slot = UdpRecvSlot {
        buf_ptr: recv_buf.as_mut_ptr(),
        buf_len: recv_buf.len(),
        recv_len: 0,
        addr: unsafe { std::mem::zeroed::<libc::sockaddr_storage>() },
        addr_len: 0,
    };
    let fd = std::os::fd::AsRawFd::as_raw_fd(&server);

    let mut reactor = UringReactor::new(128).expect("create uring reactor");
    let token = Reactor::submit(
        &mut reactor,
        NetOp::UdpRecvBatch {
            fd,
            slots_ptr: &mut slot as *mut UdpRecvSlot,
            slots_len: 1,
            flags: libc::MSG_DONTWAIT,
            prefer_multishot: true,
        },
    )
    .expect("submit udp recv batch");
    reactor.submit_pending().expect("submit pending");

    let mut got = None;
    for _ in 0..256 {
        let events = reactor.poll(true).expect("poll");
        for event in events {
            let NetEvent::OpComplete {
                token: done,
                kind,
                result,
                ..
            } = event;
            if done == token {
                assert_eq!(kind, NetOpKind::UdpRecvBatch);
                got = Some(result);
                break;
            }
        }
        if got.is_some() {
            break;
        }
    }
    assert_eq!(got, Some(1));
    assert_eq!(slot.recv_len, payload.len());
    assert_eq!(&recv_buf[..payload.len()], payload);
    assert!(slot.addr_len > 0);
}
