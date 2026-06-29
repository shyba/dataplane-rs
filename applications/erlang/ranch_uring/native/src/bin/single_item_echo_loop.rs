use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

const DEFAULT_ITERS: usize = 1_000_000;
const DEFAULT_BYTES: usize = 512;

fn main() {
    let iters = parse_env_usize("SINGLE_ECHO_ITERS", DEFAULT_ITERS);
    let bytes = parse_env_usize("SINGLE_ECHO_BYTES", DEFAULT_BYTES);
    let barrier = Arc::new(Barrier::new(2));

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let server_barrier = barrier.clone();

    let server = thread::Builder::new()
        .name("single-echo-server".into())
        .spawn(move || run_server(listener, bytes, server_barrier))
        .expect("spawn server");

    let mut client = TcpStream::connect(addr).expect("connect client");
    client.set_nodelay(true).expect("client nodelay");
    barrier.wait();

    let tx_buf = vec![0x5a; bytes];
    let mut rx_buf = vec![0u8; bytes];

    let start = Instant::now();
    for _ in 0..iters {
        client.write_all(&tx_buf).expect("client write");
        client.read_exact(&mut rx_buf).expect("client read");
    }
    let elapsed = start.elapsed();

    drop(client);
    server.join().expect("join server");

    let round_trips_per_sec = iters as f64 / elapsed.as_secs_f64();
    let bytes_per_sec = (iters * bytes * 2) as f64 / elapsed.as_secs_f64();
    println!(
        "single_item_echo_loop iters={} bytes={} elapsed_ms={:.2} round_trips_per_sec={:.0} bytes_per_sec={:.0}",
        iters,
        bytes,
        elapsed.as_secs_f64() * 1000.0,
        round_trips_per_sec,
        bytes_per_sec
    );
}

fn run_server(listener: TcpListener, bytes: usize, barrier: Arc<Barrier>) {
    let (mut stream, _) = listener.accept().expect("accept client");
    stream.set_nodelay(true).expect("server nodelay");
    barrier.wait();

    let mut buf = vec![0u8; bytes];
    loop {
        match stream.read_exact(&mut buf) {
            Ok(()) => {
                stream.write_all(&buf).expect("server write");
            }
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionReset => break,
            Err(err) => panic!("server read failed: {err}"),
        }
    }
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}
