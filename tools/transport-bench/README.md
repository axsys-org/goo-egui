# Goo transport benchmark

This compares the current domain **protobuf v2** path against JSON. Use a
compatible Reaver checkout as described in
[Server compatibility](../../CONTRACTS.md#server-compatibility); the runner defaults
to sibling `../reaver`, or accepts `--reaver /path/to/reaver`.

From the egui repository root (the commands below use the macOS wrapper):

```sh
./scripts/cargo-macos.sh build --release --locked \
  --manifest-path tools/transport-bench/Cargo.toml --target-dir target
python3 scripts/benchmark-transport.py --reaver ../reaver --samples 40 --warmup 5 --port 8210
```

The Python runner owns a fresh disposable rich-input Reaver instance, refuses
occupied ports, sends no actions, and terminates its server process group. It
uses the working Reaver checkout, its default P6 compiler and `PL_CODECACHE=1`.
Cold startup and first requests are recorded separately, not included in warm
results. Captures and raw results go to `target/transport-benchmark` by default.

HTTP measurement:

- Seven cases: rich ordinary view at two widths, two-tab deck,
  source membership, paged tree, status, and unresolved receipt.
- Five warmups per format per case, then 40 pairs per case by default.
- Seeded shuffled case order; independently balanced shuffled format order for
  each case (20 JSON-first, 20 protobuf-first pairs).
- One request at a time, loopback, uncompressed HTTP. Both formats use the same
  server and namespace; response bytes must remain unchanged throughout.
- Record total latency through full body receipt, time to response headers, and
  body-read time. Report p50/p95, raw samples, and a paired-median difference with
  a percentile bootstrap interval. This measures preparation + encoding + HTTP;
  it is not an isolated encoder benchmark or a WAN throughput test.

Client measurement:

- The release executable uses the application library and its actual typed
  Presentation, Source and Snapshot models.
- Assert domain parity before timing (optional null/absence and empty repeated
  fields normalized; HTML excluded; decimal-to-binary64 tolerance 1e-10).
- Decode directly into the same typed domain model. Warm 64 times, then run
  31 alternating-order batches of 128 decodes. Each timed iteration includes
  dropping the decoded value. Black-box inputs and outputs prevent elimination.
- Allocation counts are collected in a separate untimed decode using the system
  allocator. Counts include allocation/reallocation calls; allocated bytes sum
  requested sizes, including reallocation requests. They are not peak live
  memory, retained bytes, or RSS. Tracking is disabled during timing (the allocator
  wrapper still performs a relaxed boolean load).
- These timings include typed deserialization but exclude semantic validation, inspector
  JSON projection generation, mounting and painting. No server allocation count
  is inferred from the client measurements.

For just client decode measurements on already captured data:

```sh
target/release/goo-transport-bench target/transport-benchmark/captures
```

Run on an otherwise quiet machine. Bootstrap intervals describe this run's
samples; they do not account for systematic machine/runtime/workload differences.

The historical `evidence/transport-benchmark-run1.json` and `run2.json` reports
measured the rejected generic v1 prototype. Those files and the reported
`evidence/transport-benchmark-v2.json` are not included in this checkout. The latest
recorded v2 numbers and their qualifications are in
[VERIFICATION.md](../../VERIFICATION.md#domain-protobuf-v2--october-22-2026);
raw samples must be regenerated to inspect or independently confirm them. A fresh
run writes HTTP results to `target/transport-benchmark/http.json`, client decode
results to `decode.json`, and responses to `captures/` (or under your `--output`
directory).
Ordinary v2 preparation also avoids producing unused HTML; HTTP results measure
that whole native response path, not isolated protobuf serialization.
