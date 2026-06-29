use std::time::Instant;

const DEFAULT_TOTAL_OPS: usize = 20_000_000;
const DEFAULT_SUBMIT_CAP: usize = 16;
const DEFAULT_EXEC_CAP: usize = 8;

#[derive(Clone, Copy)]
enum Lane {
    Submit,
    Exec,
}

#[derive(Clone, Copy)]
enum Stream {
    A,
    B,
    Bg,
}

#[derive(Clone, Copy)]
struct Op {
    lane: Lane,
    stream: Stream,
    seq: u32,
    linked: bool,
}

fn main() {
    let total_ops = parse_env_usize("LINKED_BENCH_TOTAL_OPS", DEFAULT_TOTAL_OPS);
    let submit_cap = parse_env_usize("LINKED_BENCH_SUBMIT_CAP", DEFAULT_SUBMIT_CAP).max(1);
    let exec_cap = parse_env_usize("LINKED_BENCH_EXEC_CAP", DEFAULT_EXEC_CAP).max(1);

    println!(
        "linked_stream_interleave_bench total_ops={} submit_cap={} exec_cap={}",
        total_ops, submit_cap, exec_cap
    );

    run_case("linked_only", total_ops, submit_cap, exec_cap, false);
    run_case("linked_mixed_bg", total_ops, submit_cap, exec_cap, true);
}

fn run_case(label: &str, total_ops: usize, submit_cap: usize, exec_cap: usize, mixed: bool) {
    let started = Instant::now();
    let (executed, checksum) = simulate(total_ops, submit_cap, exec_cap, mixed);
    let elapsed = started.elapsed();
    let ops_per_sec = executed as f64 / elapsed.as_secs_f64();
    println!(
        "case={} executed={} elapsed_ms={:.2} ops_per_sec={:.0} checksum={}",
        label,
        executed,
        elapsed.as_secs_f64() * 1000.0,
        ops_per_sec,
        checksum
    );
}

fn simulate(total_ops: usize, submit_cap: usize, exec_cap: usize, mixed: bool) -> (usize, u64) {
    let mut submit = Vec::with_capacity(submit_cap);
    let mut exec = Vec::with_capacity(exec_cap);
    let mut count = 0usize;
    let mut checksum = 0u64;
    let mut seq_a = 1u32;
    let mut seq_b = 1u32;
    let mut seq_bg = 1u32;

    let flush =
        |submit: &mut Vec<Op>, exec: &mut Vec<Op>, count: &mut usize, checksum: &mut u64| {
            for op in submit.drain(..) {
                apply_op(op, count, checksum);
            }
            for op in exec.drain(..) {
                apply_op(op, count, checksum);
            }
        };

    for i in 0..total_ops {
        let op = if mixed {
            match i % 10 {
                0 => {
                    let v = Op {
                        lane: Lane::Submit,
                        stream: Stream::A,
                        seq: seq_a,
                        linked: true,
                    };
                    seq_a += 1;
                    v
                }
                1 => {
                    let v = Op {
                        lane: Lane::Exec,
                        stream: Stream::Bg,
                        seq: seq_bg,
                        linked: false,
                    };
                    seq_bg += 1;
                    v
                }
                2 => {
                    let v = Op {
                        lane: Lane::Submit,
                        stream: Stream::Bg,
                        seq: seq_bg,
                        linked: false,
                    };
                    seq_bg += 1;
                    v
                }
                3 => {
                    let v = Op {
                        lane: Lane::Exec,
                        stream: Stream::B,
                        seq: seq_b,
                        linked: true,
                    };
                    seq_b += 1;
                    v
                }
                4 => {
                    let v = Op {
                        lane: Lane::Submit,
                        stream: Stream::Bg,
                        seq: seq_bg,
                        linked: false,
                    };
                    seq_bg += 1;
                    v
                }
                5 => {
                    let v = Op {
                        lane: Lane::Exec,
                        stream: Stream::A,
                        seq: seq_a,
                        linked: true,
                    };
                    seq_a += 1;
                    v
                }
                6 => {
                    let v = Op {
                        lane: Lane::Exec,
                        stream: Stream::Bg,
                        seq: seq_bg,
                        linked: false,
                    };
                    seq_bg += 1;
                    v
                }
                7 => {
                    let v = Op {
                        lane: Lane::Submit,
                        stream: Stream::B,
                        seq: seq_b,
                        linked: true,
                    };
                    seq_b += 1;
                    v
                }
                8 => {
                    let v = Op {
                        lane: Lane::Submit,
                        stream: Stream::A,
                        seq: seq_a,
                        linked: true,
                    };
                    seq_a += 1;
                    v
                }
                _ => {
                    let v = Op {
                        lane: Lane::Exec,
                        stream: Stream::B,
                        seq: seq_b,
                        linked: true,
                    };
                    seq_b += 1;
                    v
                }
            }
        } else {
            match i % 6 {
                0 => {
                    let v = Op {
                        lane: Lane::Submit,
                        stream: Stream::A,
                        seq: seq_a,
                        linked: true,
                    };
                    seq_a += 1;
                    v
                }
                1 => {
                    let v = Op {
                        lane: Lane::Exec,
                        stream: Stream::B,
                        seq: seq_b,
                        linked: true,
                    };
                    seq_b += 1;
                    v
                }
                2 => {
                    let v = Op {
                        lane: Lane::Exec,
                        stream: Stream::A,
                        seq: seq_a,
                        linked: true,
                    };
                    seq_a += 1;
                    v
                }
                3 => {
                    let v = Op {
                        lane: Lane::Submit,
                        stream: Stream::B,
                        seq: seq_b,
                        linked: true,
                    };
                    seq_b += 1;
                    v
                }
                4 => {
                    let v = Op {
                        lane: Lane::Submit,
                        stream: Stream::A,
                        seq: seq_a,
                        linked: true,
                    };
                    seq_a += 1;
                    v
                }
                _ => {
                    let v = Op {
                        lane: Lane::Exec,
                        stream: Stream::B,
                        seq: seq_b,
                        linked: true,
                    };
                    seq_b += 1;
                    v
                }
            }
        };

        if op.linked {
            flush(&mut submit, &mut exec, &mut count, &mut checksum);
            apply_op(op, &mut count, &mut checksum);
            continue;
        }

        match op.lane {
            Lane::Submit => {
                if submit.len() == submit_cap {
                    flush(&mut submit, &mut exec, &mut count, &mut checksum);
                }
                submit.push(op);
            }
            Lane::Exec => {
                if exec.len() == exec_cap {
                    flush(&mut submit, &mut exec, &mut count, &mut checksum);
                }
                exec.push(op);
            }
        }
    }

    flush(&mut submit, &mut exec, &mut count, &mut checksum);
    (count, checksum)
}

#[inline(always)]
fn apply_op(op: Op, count: &mut usize, checksum: &mut u64) {
    let stream = match op.stream {
        Stream::A => 0xA1u64,
        Stream::B => 0xB2u64,
        Stream::Bg => 0xC3u64,
    };
    let lane = match op.lane {
        Lane::Submit => 0x11u64,
        Lane::Exec => 0x22u64,
    };
    *checksum = checksum
        .wrapping_mul(1_099_511_628_211)
        .wrapping_add(stream ^ lane ^ op.seq as u64);
    *count += 1;
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}
