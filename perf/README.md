# Performance Suite

Run the quick suite (recommended on every change):

```bash
cargo run --bin perf_suite
```

Run the full suite (heavier LOD0 coverage):

```bash
cargo run --bin perf_suite -- --full
```

Outputs:

- `perf/metrics_latest.csv`: last run, per-scenario metrics + theoretical ceilings.
- `perf/metrics_latest_run_<run#>_<git-hash>.csv`: archived previous `metrics_latest.csv` from each run (never overwritten).
- `perf/metrics_history.csv`: append-only summary for trend tracking.
- `perf/metrics_history.csv.legacy_*`: auto backups when history schema changes.

Key metrics:

- Chunk load time and throughput (`elapsed_ms`, `chunks_per_sec`).
- Vertex throughput (`vertices_per_sec`).
- World query throughput (`query_qps`) as a proxy for collision/query workload.
- Maximum theoretical throughput (`theoretical_chunks_per_sec`, `theoretical_vertices_per_sec`) based on measured peak and hardware thread count.
