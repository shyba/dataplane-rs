# Ranch Uring Erlang Benches

Bench modules are loaded directly from `_build/default/lib/ranch_uring/bench` after compilation.

## Stop convergence (short, repeatable)

```bash
erl -noshell \
  -pa _build/default/lib/ranch_uring/ebin \
  -pa _build/default/lib/ranch_uring/bench \
  -eval 'bench_stop_convergence:run(), init:stop().'
```

This scenario runs a short one-shot-like accept-load workload, triggers stop, then waits a bounded drain window to report how many accepts landed before/after stop.

Environment overrides:

- `BENCH_STOP_WORKERS` (default: 24)
- `BENCH_STOP_REQUESTS_PER_WORKER` (default: 200)
- `BENCH_STOP_MSG_SIZE` (default: 64)
- `BENCH_STOP_ACCEPT_SUBS` (default: 4)
- `BENCH_STOP_TRIGGER_AFTER_MS` (default: 120)
- `BENCH_STOP_DRAIN_MS` (default: 500)
- `BENCH_STOP_CLIENT_COLLECT_MS` (default: 8000)
