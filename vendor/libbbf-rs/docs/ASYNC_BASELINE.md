# Async API Baseline

This is the synchronous baseline for the optional `bbf-tokio` work described in
[`ASYNC_IMPLEMENTATION_PLAN.md`](ASYNC_IMPLEMENTATION_PLAN.md). It is a local
measurement record, not a cross-machine performance claim.

## Reproduction

Environment:

- Date: 2026-09-10
- Commit: `b34d6f4`
- Host: arm64 Darwin, `Darwin 25.6.0`; CPU and memory sysctl values were
  unavailable in the sandbox
- Toolchain/build: pinned Nix flake, release artifacts from `nix build`
- Durability: buffered writes and flush; no explicit `fsync`
- Cache condition: each run used the normal host filesystem cache; no cache
  eviction was attempted
- Peak memory: not measured for the original synchronous baseline; a separate
  process-level async resource sample is recorded below

Build the release drivers:

```text
nix build .#bbf-bench --out-link .tmp/bbf-bench-baseline-b34d6f4
nix build .#bbfmux --out-link .tmp/bbfmux-baseline-b34d6f4
```

The benchmark driver generates its own 4 KiB, 4 MiB, and 40 MiB payloads. The
reader fixture was generated with:

```text
mkdir -p .tmp/async-baseline-b34d6f4/pages
dd if=/dev/zero of=.tmp/async-baseline-b34d6f4/pages/large.dat bs=1m count=40
.tmp/bbfmux-baseline-b34d6f4/bin/bbfmux \
  .tmp/async-baseline-b34d6f4/pages \
  .tmp/async-baseline-b34d6f4/large.bbf
```

Representative commands use the release binary directly after the Nix build:

```text
.tmp/bbf-bench-baseline-b34d6f4/bin/bbf-bench OPERATION --iterations=N
.tmp/bbf-bench-baseline-b34d6f4/bin/bbf-bench reader-verify \
  --input=.tmp/async-baseline-b34d6f4/large.bbf --iterations=5
```

## Measured results

Times are nanoseconds per operation from the benchmark driver's
`per_iteration_ns` field. Iteration counts are included so the runs can be
repeated at the same operation boundary.

| Operation | Iterations | ns/op | Scope |
|---|---:|---:|---|
| `writer-add-file-4kb` | 30 | 74,630.57 | create builder, hash and stream one file |
| `writer-add-file-4mb` | 10 | 2,011,583.40 | create builder, hash and stream one file |
| `writer-add-file-40mb` | 3 | 15,404,097.33 | create builder, hash and stream one file |
| `writer-dedup-file-4kb` | 30 | 8,927.77 | reread/hash duplicate file |
| `writer-dedup-file-4mb` | 10 | 257,554.10 | reread/hash duplicate file |
| `writer-dedup-file-40mb` | 3 | 3,825,000.00 | reread/hash duplicate file |
| `writer-write-4kb` | 30 | 129,505.57 | in-memory build plus publication |
| `writer-write-4mb` | 10 | 956,054.10 | in-memory build plus publication |
| `writer-write-40mb` | 3 | 43,112,333.33 | in-memory build plus publication |
| `writer-file-4kb` | 30 | 131,502.77 | read source, build, publish |
| `writer-file-4mb` | 10 | 1,072,066.70 | read source, build, publish |
| `writer-file-40mb` | 3 | 11,181,708.33 | read source, build, publish |
| `writer-add-4kb` | 30 | 1,790.27 | in-memory unique asset |
| `writer-add-4mb` | 10 | 243,883.30 | in-memory unique asset |
| `writer-add-40mb` | 3 | 4,339,513.67 | in-memory unique asset |
| `writer-dedup-4mb` | 10 | 189,441.70 | in-memory duplicate |
| `writer-dedup-40mb` | 3 | 2,689,236.00 | in-memory duplicate |

Reader measurements use the generated 40 MiB single-asset archive:

| Operation | Iterations | ns/op |
|---|---:|---:|
| `reader-open` | 30 | 10,455.57 |
| `reader-header` | 10,000 | 1.04 |
| `reader-footer` | 10,000 | 2.07 |
| `reader-asset-lookup` | 10,000 | 2.42 |
| `reader-string` | 10,000 | 1.14 |
| `reader-asset-hash` | 10 | 1,176,650.00 |
| `reader-verify` | 5 | 1,396,716.60 |

The synchronous driver does not measure multiple distinct file inputs, large
multi-record indexes, concurrent operations, range reads, peak RSS, or timer
latency. Those remain separate async measurements and are not silently
compared to the synchronous operation boundaries.

## Initial async measurements

The first `bbf-tokio` benchmark driver is now reproducible with the same Nix
development environment:

```text
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  build-file SOURCE DESTINATION 3
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  dedup-file SOURCE DESTINATION 10
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  append-file BASE_ARCHIVE NEW_ASSET 3
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  petrify NORMAL_ARCHIVE PETRIFIED_DESTINATION 3
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  open ARCHIVE 30
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  verify ARCHIVE 5
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  read ARCHIVE 10
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  range ARCHIVE 100
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  concurrent ARCHIVE 10 8
```

Measured against the existing 40 MiB single-asset archive on the same host,
from the benchmark-driver worktree based on `d6d14c4`:

| Operation | Iterations | ns/op | Timer max lag |
|---|---:|---:|---:|
| `build-file` | 3 | 71,154,680.67 | not sampled |
| `dedup-file` (4 MiB) | 10 | 275,437.50 | not sampled |
| `dedup-file` (40 MiB) | 3 | 4,359,305.67 | not sampled |
| `open` | 30 | 16,515.27 | not sampled |
| `verify` | 5 | 3,948,650.07 | not sampled |
| `read` | 10 | 5,104,545.80 | not sampled |
| `range` (up to 64 KiB) | 100 | 7,537.91 | not sampled |
| `concurrent` (8 ranges) | 10 | 200,183.30 | 688,500 ns |

These are operation-boundary measurements, not yet speed-parity claims:
`build-file` includes staging, finalization, publication, and reopen; `open`
decodes the index while the synchronous `reader-open` baseline maps it; and
async verification now uses one coarse blocking operation with a reusable
bounded `BufReader`; the matching synchronous baseline uses the mapped reader.
The buffered change reduced the async 40 MiB verification sample from
14,417,958 ns/op to 3,948,650 ns/op (about 3.65x faster), while the current
synchronous sample is 1,579,783 ns/op. Async verification is therefore still
about 2.5x slower than the synchronous baseline and remains an investigation
item, not a hidden optimization.

The duplicate-file slices keep the first ingestion outside the timed region
and measure repeated source hashing plus dedup lookup. The matching synchronous
baselines are 257,554.10 ns/op for 4 MiB and 3,825,000.00 ns/op for 40 MiB;
the async results are therefore approximately 1.07x and 1.14x slower,
respectively, with no additional cache or task pipeline.

## Async benchmark matrix

Commit `8d842ae` adds a deterministic matrix driver. It generates fixtures
outside measured regions, uses buffered writes and flushes without `fsync`,
and runs on the normal host filesystem cache:

```text
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release \
  --example matrix -- 3
```

The three-iteration sample below was run on the same arm64 Darwin host and
reports nanoseconds per operation. The matrix uses a two-worker Tokio runtime
so the periodic timer can observe executor stalls during direct bounded
streaming reads. Unique/duplicate construction uses four
records for 4 KiB and 4 MiB inputs and two records for 40 MiB inputs. The
large-index archive contains 64 distinct 4 KiB assets.

| Operation | ns/op | Timer max lag |
|---|---:|---:|
| `small-unique` | 314,805 | 0 ns |
| `small-duplicate` | 283,861 | 0 ns |
| `medium-unique` | 21,248,903 | 0 ns |
| `medium-duplicate` | 6,183,667 | 0 ns |
| `large-unique` | 104,766,486 | 0 ns |
| `large-duplicate` | 58,609,847 | 0 ns |
| `open-small-index` | 31,611 | 0 ns |
| `open-large-index` | 27,597 | 0 ns |
| `verify-large` | 8,255,055 | 0 ns |
| `read-large` | 4,582,694 | 0 ns |
| `stream-large` | 4,414,917 | 0 ns |
| `range-large` | 16,278 | 0 ns |
| `concurrent-unlimited` | 574,403 | 578,875 ns |
| `concurrent-limited-4` | 538,028 | 0 ns |

This matrix establishes operation boundaries and executor responsiveness; it
does not measure peak RSS. The bounded `stream-large` operation uses one
reusable 256 KiB buffer and does not accumulate the payload. In the matched
three-iteration run at commit `6e3786f`, it measured 4,414,916.67 ns/op versus
4,517,694.67 ns/op for `read-large`; this is a small operation-boundary
observation, not a general speed claim because the allocating and streaming
paths have different output contracts. The limited concurrent result is not
treated as a speed improvement: it is a contention/latency observation for the
caller-owned limiter. Matching synchronous measurements for each boundary
remain required before making speed-parity claims.

The corrected two-worker matrix at commit `788cc3b` measured `stream-large` at
2,960,347 ns/op with a maximum timer lag of 49,625 ns, versus `read-large` at
4,468,430 ns/op. This is a separate cache-sensitive sample, not a universal
speed claim; it records the direct positional reader's executor-stall tradeoff
with a timer worker that can run concurrently.

A fresh three-iteration matrix at commit `09b13e1` measured the following on
the same arm64 Darwin host. It supersedes neither earlier sample; the values
show normal filesystem-cache variance and provide the current reproducible
checkpoint:

| Operation | ns/op | Timer max lag |
|---|---:|---:|
| `small-unique` | 281,930 | 0 ns |
| `small-duplicate` | 264,125 | 0 ns |
| `medium-unique` | 21,004,806 | 0 ns |
| `medium-duplicate` | 6,135,222 | 0 ns |
| `large-unique` | 106,364,167 | 0 ns |
| `large-duplicate` | 57,629,472 | 0 ns |
| `open-small-index` | 46,417 | 0 ns |
| `open-large-index` | 28,764 | 0 ns |
| `verify-large` | 8,777,000 | 0 ns |
| `read-large` | 4,647,722 | 0 ns |
| `stream-large` | 3,378,764 | 304,417 ns |
| `range-large` | 34,056 | 0 ns |
| `concurrent-unlimited` | 598,319 | 583,333 ns |
| `concurrent-limited-4` | 519,056 | 0 ns |

The current stream sample remains faster than the allocating read operation
at this boundary, but its direct positional reads produced a measurable
0.304 ms maximum timer lag. The concurrent range measurements show the
caller-owned limiter tradeoff; neither result is counted as a speedup over the
synchronous implementation without a matching operation boundary.

## Async sealed-file editing sample

The new editing slices were measured on the same arm64 Darwin host against the
40 MiB `large.bbf` fixture. The append setup copied three base archives before
the timed region, so the result covers async open, a new 4 MiB file-backed asset
append, page append, and finalization rather than fixture preparation.

| Operation | Iterations | ns/op | Timer max lag |
|---|---:|---:|---:|
| `append-file` | 3 | 4,526,791.67 | 0 ns |
| `petrify` | 3 | 47,925,958.33 | 0 ns |

The petrification sample streams the existing 40 MiB archive to a staged
destination and publishes it after completion. The initial values are first
boundary measurements, not speed-parity claims; the matched warmed comparison
below is the current petrification datapoint.

A warmed paired rerun added a synchronous baseline at the same boundary:
`writer-append-file-4mb` measured 1,875,666.67 ns/op, while async
`append-file` measured 1,970,166.67 ns/op. The async/sync ratio was about 1.05x,
inside the ten-percent investigation threshold; no append-path optimization is
justified by this sample.

A matched warmed petrification run used the same 40 MiB normal-layout input and
five iterations. Synchronous `writer-petrify-40mb` measured 12,916,033.40
ns/op; async `petrify` measured 13,791,791.60 ns/op with zero measured 1 ms
timer lag. The async/sync ratio was about 1.07x, inside the ten-percent
investigation threshold; no petrification-path optimization is justified by
this sample.

## Async resource sample

The matrix was run a second time after compilation with the host resource
reporter so compiler memory was not part of the sample:

```bash
TMPDIR=$PWD/.tmp /usr/bin/time -l nix develop -c cargo run -p bbf-tokio \
  --release --example matrix -- 1
```

On the same arm64 Darwin host, the process at commit `0dfaf9a` reported a
maximum resident set size of `47,087,616` bytes (about 44.9 MiB). This covers
the complete matrix, including its generated 40 MiB fixtures and concurrent
workload, and is not a claim about the memory cost of any single operation. A
future memory slice should measure build, open, and read operations
independently if peak RSS becomes a performance target.
