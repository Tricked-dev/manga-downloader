# Benchmarking

The Rust benchmark driver is available through the pinned flake:

```text
nix run .#bbf-bench -- reader-verify --input=FILE --iterations=1000
nix run .#bbf-bench -- writer-constructor --iterations=100
nix run .#bbf-bench -- writer-add-file-4kb --iterations=100
nix run .#bbf-bench -- writer-add-file-4mb --iterations=20
nix run .#bbf-bench -- component-hash-4mb --iterations=1000
nix run .#bbf-bench -- component-write-4mb --iterations=100
nix run .#bbf-bench -- component-dedup-lookup-4mb --iterations=1000001
nix run .#bbf-bench -- component-page-index-4mb --iterations=1001
nix run .#bbf-bench -- component-page-index-hash-4mb --iterations=1001
nix run .#bbf-bench -- writer-add-4kb --iterations=100
nix run .#bbf-bench -- writer-write-4mb --iterations=10
nix run .#bbf-bench -- writer-file-4mb --iterations=10
nix run .#bbf-bench -- writer-add-40mb --iterations=3
nix run .#bbf-bench -- writer-dedup-40mb --iterations=3
nix run .#bbf-bench -- writer-add-meta --iterations=1000
nix run .#bbf-bench -- writer-add-section-parent --iterations=1000
nix run .#bbf-bench -- writer-dedup-file-4mb --iterations=100
nix run .#bbf-bench -- writer-dedup-file-4kb --iterations=1000
nix run .#bbf-bench -- writer-append-file-4mb --iterations=3
nix run .#bbf-bench -- writer-petrify-40mb --iterations=5

# Async sealed-file editing and petrification boundaries
nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  append-file BASE_ARCHIVE.BBF NEW_ASSET 10
nix develop -c cargo run -p bbf-tokio --release --example bench -- \
  petrify NORMAL_ARCHIVE.BBF PETRIFIED_ARCHIVE.BBF 10
```

Its operation names mirror the relevant C++ benchmark names and its output is
machine-readable (`operation`, `iterations`, `total_ns`, `per_iteration_ns`,
and `checksum`). The reader operations keep one read-only mmap alive so the
measurement covers BBF parsing and hashing without repeatedly copying the
file. The benchmark explicitly opts into `MappedFile`'s unsafe immutability
contract; production callers that cannot guarantee an unchanged input can use
the safe owned-file reader instead.

For CLI parity, build both standalone binaries and compare identical commands
with the Rust-based `hyperfine` package:

```text
nix build .#libbbf-cpp --out-link /tmp/libbbf-cpp-result
nix build .#bbfmux --out-link /tmp/libbbf-rust-result
nix shell nixpkgs#hyperfine -c hyperfine \
  '/tmp/libbbf-cpp-result/bin/bbfmux INPUT OUTPUT_CPP --verify' \
  '/tmp/libbbf-rust-result/bin/bbfmux INPUT OUTPUT_RUST --verify'
```

The 2026-09-10 arm64-darwin smoke run used the checked-in C++ fixture and 15
hyperfine runs. Verification measured 0.941 ms for C++ versus 1.3 ms for Rust
(1.43x C++/Rust); muxing the small compatibility fixture measured 1.1 ms versus
1.5 ms (1.30x). These include process startup and are baselines, not
algorithmic speed conclusions.

The same mux command over the 40 MiB `largeAsset.png` reference input measured
285.8 ms for C++ versus 311.5 ms for Rust (1.09x C++/Rust, five runs). The
resulting BBF files had the same SHA-256 digest, so this comparison includes
the full payload path without a format mismatch.

A fresh release-artifact smoke run on the same 40 MiB single-file fixture
measured 23.0 ms for C++ versus 21.4 ms for Rust (Rust 1.08x faster, five
runs; standard deviation was 1.1 ms and 2.6 ms respectively). Both outputs
had SHA-256 `c5bb10f5f98e5a7a3afdf3579b830c3805327d176f409b080e93e3c7de77a3db`.
The variance is large enough that this is an additional datapoint, not a
replacement for the earlier baseline.

The Rust writer now preallocates the payload and index envelopes before
serialization. The pinned `bbf-bench` smoke measurements after that change
were 1.259 µs/op for `writer-add-4kb` (1,000 iterations) and 0.527 ms/op for
`writer-add-4mb` (20 iterations). These are local Rust-only measurements; the
fixture compatibility tests remain the byte-level parity gate.

The driver also isolates buffered file output: `writer-write-4kb` measured
149.59 µs/op (10 iterations) and `writer-write-4mb` measured 1.057 ms/op (3
iterations). These include creating, serializing, and writing the temporary BBF
file, while cleanup happens outside the timed region.

The complete file-output path, including source-file read, XXH3 hashing, index
serialization, and finalization, measured 165.16 µs/op for `writer-file-4kb`
(10 iterations) and 1.120 ms/op for `writer-file-4mb` (3 iterations). The
separate `writer-add-file-4kb` and `writer-add-file-4mb` operations now isolate
the reference builder's
`addPage` path without finalization.

A paired 20-iteration run against the pre-index-streaming commit measured
`writer-write-4mb` at 1.117 ms/op before versus 0.976 ms/op after (12.7%
lower), and `writer-file-4mb` at 1.936 ms/op before versus 1.576 ms/op after
(18.6% lower). The existing byte-for-byte fixture tests remain the correctness
gate for this allocation change.

The Rust driver also covers the C++ metadata and section writer cases with the
same setup outside the timed region. The fresh C++ sample measured
`writer-add-meta` at 17.98 ns/op, `writer-add-meta-parent` at 32.33 ns/op,
`writer-add-section` at 9.89 ns/op, and `writer-add-section-parent` at 18.54
ns/op. The Rust baseline was 18.19, 32.93, 10.23, and 23.00 ns/op
respectively. After inlining the section wrappers and string-pool call, longer
Rust parented-section runs measured 21.90–22.38 ns/op; this is a modest,
repeatable direction but still leaves the parented section case about 1.18x
slower. No cache, async runtime, or changed behavior is involved.

The driver also covers 40 MiB in-memory add and deduplication cases as
`writer-add-40mb` and `writer-dedup-40mb`. They remain separate from the exact
file-backed paths. The new `writer-dedup-file-4mb` operation matches the C++
duplicate-page boundary: the first file-backed add is outside the timer, and
each measured iteration rereads and hashes the same source file through the
existing builder. Rust measured 245.91–248.04 µs/op over three 100-iteration
runs versus 437.58 µs/op for the repaired C++ benchmark (30 samples), about
1.77x faster. This is a synchronous file/hash path with no additional cache.
The corresponding `writer-dedup-file-4kb` slice measured 7.95–8.11 µs/op in
three 1,000-iteration Rust runs versus 9.13 µs/op for C++ (30 samples), about
1.15x faster.

`writer-constructor` measures creation and close of the Rust file-backed
builder, which is the stable analogue of the reference's
`BBFWriter - Constructor` benchmark.

`writer-add-file-4kb` and `writer-add-file-4mb` are exact file-backed writer
analogues: each creates the builder, hashes and streams one source file through
`add_page`, and closes the builder inside each timed iteration, matching the
reference's `BBFWriter - Add Page` measurement boundary. The existing
`writer-file-4kb` operation intentionally remains a complete finalized-file
path and is not used for this direct reference comparison.

Reader-side baselines are now exposed too. Against the mapped 40 MiB BBF
fixture, the current run measured 1.02 ns/op for `reader-header`, 5.96 ns/op
for `reader-footer`, 2.67 ns/op for indexed `reader-asset-lookup`, 4.79 ns/op
for indexed `reader-string`, and 964.89 µs/op for `reader-asset-hash` (100,000,
100,000, 10,000, 10,000, and 100 iterations respectively). Full
`reader-verify` measured 1.292 ms/op over 10 iterations. Indexed lookup
measurements reuse one decoded footer per operation.
The CLI verification path using the same view measured 4.8 ms versus 4.9 ms
for the previous Rust CLI (five runs; startup dominates this small command).

A paired 100,000-iteration run against the pre-cache commit measured
`reader-header` at 8.17 ns/op before versus 1.02 ns/op after (87.5% lower),
and `reader-footer` at 60.34 ns/op before versus 5.96 ns/op after (90.1%
lower). The direct reader now caches successfully decoded immutable header and
footer values; malformed inputs still return their parse errors.

The CLI now uses the shared buffered streaming serializer. On a dedicated
40 MiB single-asset input, five hyperfine runs measured 18.0 ms for C++ versus
18.6 ms for Rust (1.04x C++/Rust); both output files had the same SHA-256
digest.

CLI petrification now streams the index and payload ranges instead of reading
the complete input into a second buffer. On the same 40 MiB BBF, five runs
measured 17.1 ms for the previous Rust path, 15.0 ms for the streaming Rust
path, and 16.3 ms for C++ (noisy local run); all three outputs had the same
SHA-256 digest.

On the same mapped fixture, the full `--info` report measured 1.4 ms for C++
versus 1.7 ms for Rust (15 runs; startup and mapping dominate). Full
`--extract` measured 10.9 ms versus 14.6 ms (10 runs, with a noisy Rust range);
the metadata report, hash report, and extracted payload were byte-identical,
including matching SHA-256 digests for the payload.

The pinned C++ Catch2 benchmark target is built as `result/bin/bbfbench`. Its
Nix package applies the tracked harness patch
`patches/libbbf-bbfbench-no-shallow-copies.patch`: the original benchmark
returned `BBFBuilder` and `BBFReader` by value despite shallow raw-pointer
ownership, dereferenced a missing metadata entry during reader setup, and used
an invalid asset index for the reader hash case. It also removes an unused
400 MiB fixture that could crash setup on a full temporary volume, and adds
checksum-preserving in-memory XXH3, buffered payload-write, asset-table-lookup,
and page-index component cases. With those benchmark-only defects corrected,
`bbfbench 'Performance Benchmarks' -r compact` completes successfully (`1`
test case passed), so the reference samples are available for speed-parity
work.

The first exact writer slice measured `BBFWriter - Add Page (4KB)` at 55.62
µs/op in the repaired C++ benchmark (100 samples) and
`writer-add-file-4kb` at 53.55 µs/op in the optimized Rust benchmark (100
iterations). Three additional Rust runs ranged from 48.71 to 58.14 µs/op, so
this is a parity baseline rather than an optimization claim.

The matching 4 MiB slice measured `BBFWriter - Add Page (4MB)` at 2.479 ms/op
in the C++ benchmark and `writer-add-file-4mb` at 1.635 ms/op in the optimized
Rust benchmark. Three additional Rust runs ranged from 1.466 to 1.591 ms/op;
this is useful evidence that the Rust streaming path is currently faster for
this workload, not yet an optimization target because the component costs have
not been decomposed.

The first component comparison isolates XXH3-128 over the same 4 MiB payload:
the repaired C++ benchmark reports 88.48 µs/op, while optimized Rust reports
about 94.01 µs/op over three 1,001-iteration runs (Rust is about 1.07x
slower). This remains CPU-bound, so async I/O is not expected to improve it;
the next optimization decision should be based on the hash implementation,
target flags, and generated code, not an async runtime. A refreshed generic
Rust run measured 97.93 µs/op, while a `target-cpu=native` release build
measured 97.98 µs/op, so target tuning alone showed no improvement and no
default build flag was changed.

The next component comparison isolates buffered writing of the same 4 MiB
payload, with both benchmarks opening, writing, flushing, and closing the same
relative output file. The repaired C++ benchmark measured 7.497 ms/op and
optimized Rust measured 5.837 ms/op over 100 iterations (Rust was about 1.28x
faster in this run). This is a noisy filesystem-bound datapoint, not a reason
to add async I/O: the existing synchronous buffered path is already slightly
ahead, and an async runtime would add dependencies and a different operation
boundary before it had demonstrated a benefit.

The first deduplication component probe isolates a successful open-addressed
asset-table lookup after both implementations have inserted the same precomputed
XXH3-128 hash. The repaired C++ benchmark measured 1.21 ns/op, while optimized
Rust measured 0.98 ns/op over 1,000,001 iterations (Rust was about 1.23x
faster). Hash computation and file reading are intentionally outside this
probe; the existing full `writer-dedup-4mb` operation remains the end-to-end
comparison.

The page-index probe isolates encoding and hashing 4 MiB of identical page
records. C++ copies its packed wire records and measured 166.89 µs/op in the
fresh reference run; Rust measured 184.60 µs/op on the matching optimized
build (about 1.11x slower). Rust first measured 308.84 µs/op; contiguous
batching reduced that to 220.85 µs/op, and a wire-preserving single-`u128`
record write reduced it again to about 175.02 µs/op. The remaining gap is
small enough to keep as a measured follow-up rather than introduce
native-layout or unsafe shortcuts; async I/O is not relevant to this
CPU-and-memory-bound probe.

A fresh arm64-darwin sample on 2026-09-10 measured the C++ encode-and-hash
component at 173.73 µs/op (30 samples). Three Rust release runs measured
184.72–191.74 µs/op over 1,001 iterations, or about 1.06–1.10x slower. The
matching hash-only C++ component measured 91.03 µs/op, while Rust measured
98.13–99.77 µs/op, or about 1.08–1.10x slower. These ranges confirm the
remaining CPU-bound gap but do not justify async I/O or a cache-only change;
the next candidate must preserve the 16-byte wire representation and be
validated against the same component boundary.

One portable four-record loop-unrolling candidate was tested against this
baseline. It preserved the checksum and passed the format tests, but measured
193.35–204.05 µs/op in three release runs, slower than the existing
185–192 µs/op range, so it was rejected and not retained in production.

The matching hash-only probe now measures the pre-encoded 4 MiB page index,
corresponding to the reference's `Cached Page Index Hash` benchmark without
adding a cache to the Rust implementation. A fresh Rust run measured 96.34
µs/op over 1,001 iterations; the C++ cached-hash component measured 90.76
µs/op. The combined encode-and-hash run measured 184.60 µs/op on the same
Rust build. This separates the codec cost for the next wire-preserving
optimization slice: a post-revert run measured 176.59 µs/op combined and
97.92 µs/op hash-only. Across the two Rust runs, the observed ranges are
176.59–184.60 µs/op combined and 96.34–97.92 µs/op hash-only, or roughly
1.06–1.11x slower than the fresh C++ samples. No cache is used in either Rust
probe.

A safe field-wise encoder candidate was also measured and rejected: it
regressed the combined Rust probe to 220.42 µs/op. The production `u128`
wire write remains in place.

A fresh matched release sample on the current `6834a1b` checkpoint changed
the speed-parity status of these components. Rust measured 105.83 µs/op for
`component-hash-4mb` over 1,001 iterations, while the pinned C++ benchmark's
10-sample mean was 119.42 µs/op. Rust measured 195.84 µs/op for
`component-page-index-4mb`, versus 232.02 µs/op for C++ on the same benchmark
run. The hash-only page-index slice remained near parity: 104.40 µs/op for
Rust versus 100.65 µs/op for C++. These are local same-host observations with
filesystem/cache and benchmark variance; they do not authorize a cache-only
change or eliminate the need for repeated samples.

### Current end-to-end refresh

On 2026-09-10, a fresh release run on arm64-darwin measured the synchronous
Rust file-backed boundaries as follows:

| Operation | Iterations | ns/op |
|---|---:|---:|
| `writer-file-4kb` | 30 | 199,526.37 |
| `writer-file-4mb` | 10 | 976,704.20 |
| `writer-dedup-file-4mb` | 30 | 269,147.20 |
| `writer-add-file-4kb` | 30 | 67,643.07 |
| `writer-add-file-4mb` | 10 | 2,945,850.00 |
| `writer-add-file-40mb` | 3 | 18,714,069.33 |
| `reader-open` | 30 | 8,718.07 |
| `reader-verify` | 5 | 22,691,600.00 |

The same current pinned C++ reference run (five samples, no warmup) reported
169.42 µs/op for its 4 KiB add-page case, 6.386 ms/op for 4 MiB, and 102.48
µs/op for XXH3 over 4 MiB. The C++ benchmark's add-page setup is kept as the
reference boundary; the complete Rust `writer-file-*` operations additionally
publish a finalized archive and therefore are not compared directly to that
row. These are same-host samples with normal filesystem and scheduler noise,
not universal speed claims. The async `append-file` and `petrify` drivers now
measure their open/stream/finalize and streaming-transform boundaries
independently. The matching synchronous petrification driver is
`writer-petrify-40mb`; it creates one 40 MiB normal-layout archive outside the
timed region and measures only streaming transformation and publication.

The current 40 MiB verification refresh measured the async buffered verifier at
3.892 ms/op (five iterations) versus 2.119 ms/op for the mapped synchronous
boundary. Two small candidates were measured and rejected: increasing the
reusable verification buffer from 256 KiB to 1 MiB measured 4.081 ms/op, and
replacing the buffered seek loop with positional reads also measured about
4.081 ms/op. The existing 256 KiB buffered path remains in production; the
async overhead is tracked for a future investigation rather than hidden with a
cache or an unfair operation boundary.

The synchronous benchmark driver also exposes `writer-append-file-4mb`. It
prepares a 40 MiB base archive, copies per-iteration destinations outside the
timed region, then measures open, append of a new 4 MiB file-backed asset, page
append, and finalization. A paired warmed sample measured 1.876 ms/op
synchronously and 1.970 ms/op asynchronously (about 1.05x async/sync), within
the ten-percent investigation threshold. This is a boundary datapoint, not a
general runtime claim.

A matched warmed petrification run used the same 40 MiB normal-layout input
and five iterations: synchronous `writer-petrify-40mb` measured 12.916 ms/op,
while async `petrify` measured 13.792 ms/op with zero measured 1 ms timer lag.
Async was about 1.07x synchronous, inside the ten-percent investigation
threshold; no petrification-path optimization is justified by this sample.
