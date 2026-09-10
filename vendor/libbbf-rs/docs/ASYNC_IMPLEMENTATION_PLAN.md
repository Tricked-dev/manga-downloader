# Async `libbbf-rs` Implementation Plan

Status: async foundation, sealed-file append, and acceptance coverage
implemented; end-to-end parity measurement and optimization remain in progress.

This plan defines an optional, performance-first Tokio API for web servers. It
does not replace the synchronous implementation, change the BBF wire format,
or add an internal executor. The synchronous crates remain Tokio-free, and all
async behavior is built on the caller's Tokio runtime.

## Goals and non-goals

The async implementation must preserve exact BBF fixture and wire compatibility
while improving practical server throughput, allocation behavior, and local
file I/O efficiency. Brief executor stalls of a few milliseconds are acceptable
when they avoid expensive scheduling or copying.

Do not add a custom executor, a per-archive worker thread, a channel pipeline,
or a task per record, small read, or hash update. Cancellation machinery is
deliberately modest: already-started blocking work may finish after its future
is dropped, and running work retains its concurrency permit until completion.
HTTP routing, multipart parsing, range-header parsing, and response caching
remain outside the library.

## Current implementation checkpoint

The first async foundation is implemented in the optional `bbf-tokio` crate.
The root `libbbf` facade exposes it only through its opt-in `tokio` feature;
the default facade dependency graph remains Tokio-free.
It provides staged writer creation and publication, file-backed asset
ingestion, bounded non-seekable reader ingestion, immutable archive handles,
positional asset/range reads, bounded `AsyncRead` asset/range views, structural
open validation, and full payload verification. It now also mirrors the
synchronous sealed-file `FileAppender`:
old payload bytes remain in place, while new payloads and the replacement
index/footer are appended until finalization. It supports adding, replacing,
and removing page, metadata, and section records without copying unchanged
payload ranges. It reuses the existing
`bbf-format` codecs and `bbf-mux` primitives; Tokio is not added to the
synchronous crates.
It also accepts petrified layouts and exposes `AsyncArchive::petrify_to` as a
coarse streaming transform. Reference-compatible petrified files may report a
false footer checksum because the existing transform preserves that wire
behavior; payload verification remains available. All coarse file operations
can now share a caller-owned `ConcurrencyLimiter`, so a web server can bound
blocking work across archive handles without introducing an internal worker
pool or per-record tasks.
`AsyncFileAppender::open_with_options` applies the same pre-allocation index
limit before taking its replacement-index snapshot.

The remaining work is deliberate: continue end-to-end parity measurement,
measure memory per operation where practical, and tune existing streaming
operations only against those measurements. Independent append and
petrification benchmark slices are now present; the first verification tuning
candidates were measured and rejected without changing the implementation.

The async benchmark driver now exposes `append-file` and `petrify` operation
boundaries. Their setup copies or opens fixtures outside the timed region, so
the measured work covers sealed-file open/append/finalize and the streaming
transform respectively without conflating fixture preparation with the async
operation.

The failure-path slice now has explicit short-read, injected-reader-failure,
short-write, and injected-write-failure coverage. The async writer discards a
failed non-seekable staging input without publishing it and remains usable for
subsequent valid input. Explicit writer abort cleanup is also covered and
documented; dropped futures remain intentionally modest and may leave an
already-started blocking operation running to completion.
Async archive and sealed-file appender opens also have a bounded malformed
input corpus covering truncated headers, invalid footer offsets, overflowing
string-pool ranges, and reversed index ranges; each case is checked for an
error rather than a panic.

## Work order

1. Finish shared synchronous correctness prerequisites.
2. Establish reproducible synchronous and async performance baselines.
3. Extract only the Tokio-independent validated layout, codec, bookkeeping,
   copy/hash, and publication primitives needed by both implementations.
4. Add coarse-grained async local-file construction, opening, verification,
   and petrification using `spawn_blocking` around efficient synchronous work.
5. Add concurrent positional asset and range reads with bounded allocation.
6. Add bounded-memory `AsyncRead` asset ingestion without a mandatory pipeline.
7. Add async editing and reuse of unchanged payload ranges.
8. Tune only measured bottlenecks and document tradeoffs, limitations, and
   executor blocking points.

## Phase 0: synchronous correctness gate

Before introducing Tokio:

- Keep `Builder::write_to` validation-before-touching and destination-local
  unique staging with atomic publication.
- Keep the fixed `petrified.bbf.tmp` path only in the explicit compatibility
  CLI path; normal library petrification uses unique destination-local staging.
- Define `FileBuilder` behavior after a partial append: either roll back the
  append or enter a documented failed state. Never accept a hash that does not
  describe the bytes actually stored.
- Add injected short-read, short-write, source-change, and failed-append tests.
- Preserve the existing CLI compatibility suite and all byte-level fixtures.

## Phase 1: baselines and measurement

Run the existing workspace tests, Clippy, formatting check, Nix checks, and
Rust/C++ benchmarks before changing behavior. Add reproducible release
benchmarks with commit ID, hardware, toolchain, commands, raw samples, and
durability settings for:

- small, 4 MiB, and 40 MiB file-backed assets;
- mostly unique and heavily duplicated inputs;
- small and large index opening;
- full verification;
- whole-asset and repeated small-range reads;
- several concurrent operations; and
- a periodic executor timer measuring responsiveness during async workloads.

Use synchronous performance as the baseline. A repeatable async regression
above roughly 10% is an investigation trigger, not an automatic rejection;
report throughput, elapsed time, peak memory where practical, and executor
stall tradeoffs honestly.

## Phase 2: shared primitives

Refactor only at boundaries required by the async implementation. Keep these
pieces independent of Tokio:

- validated header/footer/index/layout representation;
- index and footer encoding;
- asset deduplication and record bookkeeping;
- reusable buffered copy-and-hash loops; and
- finalization, staging, flushing, and publication helpers.

Use separate thin synchronous and asynchronous I/O drivers. Do not introduce a
generic sync/async abstraction framework, and preserve all existing public
sync APIs and fixture output.

## Phase 3: `bbf-tokio` crate and minimal API

Add an optional `bbf-tokio` crate. Existing format, reader, writer, and CLI
crates must not depend on Tokio. The new crate uses the caller's runtime and
provides cheaply cloneable archive handles with shared immutable index state:

```rust
let archive = AsyncArchive::open(path).await?;
let page = archive.page(0)?;
let bytes = archive.read_asset(page.asset_index).await?;
archive.verify().await?;

let mut writer = AsyncArchiveWriter::create(destination).await?;
let asset = writer.append_asset_file(source, media_type, flags).await?;
writer.append_page(asset, page_flags)?;
let archive = writer.finish().await?;
```

Metadata and page lookups are synchronous after open. Allocating convenience
methods such as `read_asset` are documented as such, with bounded-memory range
and streaming alternatives. Errors retain their underlying I/O and format
causes. No service object is required for basic use; callers may share a
concurrency limiter across operations.

## Phase 4: file-backed operations

Start with coarse `spawn_blocking` boundaries for construction, verification,
editing, and petrification. Reuse ownership of files, buffers, and indexes;
use sizable reusable buffers and benchmark practical sizes. Add batched file
addition where it avoids repeated state transfer:

```rust
writer.add_asset_file(path, media_type, flags).await?;
writer.add_files(files).await?;
```

The synchronous `Builder`, `FileBuilder`, and `ArchiveEditor` expose matching
`append_*` entry points. They are additive names for the same deduplicating
record operations and do not change the BBF wire layout; the async writer
provides the same operations over coarse blocking boundaries.

`finish().await` must emit the existing layout, patch the header, flush before
success, publish staged output safely, and make stronger durability optional
rather than forcing `fsync` per operation.

## Phase 5: non-seekable ingestion

Add `add_asset_reader(reader, media_type, flags)` for Tokio `AsyncRead` inputs.
Use a bounded reusable buffer and a straightforward read → incremental hash →
private staging append loop. Record actual length and only publish the asset
and page/index records after successful ingestion. Duplicate payloads discard or
truncate the uncommitted append and reference the existing asset.

Do not spawn per-chunk work. An interrupted append may initially invalidate the
writer, but that state must be explicit and must never publish incomplete data.
Introduce a producer/consumer pipeline only if measurements justify its
scheduling and copying costs.

## Phase 6: concurrent reads and verification

On open, validate header/footer/index ranges and enforce configurable
allocation limits before allocating. Keep payloads on disk and retain the
opened file in the archive handle. Use positional reads with bounded chunks;
never serialize readers behind one shared seek cursor or assume cloned handles
have independent positions. Coalesce adjacent ranges only when measurement
shows a material gain.

Provide whole-asset and range reads first. The `AsyncAssetReader` adapter
retains the opened archive file, uses positional reads with an independent
logical offset, caps reads at the requested asset range, and does not allocate
the payload or create a task per record/chunk. Its bounded regular-file reads
are performed directly from `poll_read`; callers that require strict executor
non-blocking can use the coarse `read_asset_range` operation instead.
Verification checks structural/index integrity and hashes each distinct asset
once, producing an actionable report or error.

## Phase 7: editing and modest cancellation

Async editing and petrification stream unchanged payload ranges and reuse the
shared safe staging/publication transform; sealed-file append editing now
supports add/replace/remove index records without wrapping the whole archive
in memory. Dropped futures may leave already-started blocking jobs running.
The direct bounded reader's executor-stall tradeoff, malformed-open corpus,
independent multi-asset read coverage, and editing/petrification boundaries
are now recorded. Remaining work is repeated end-to-end comparison, memory
measurement, and tuning only where a candidate beats the current boundary
without changing wire bytes or cancellation semantics.
Provide explicit abort/cleanup where useful, prevent incomplete publication,
and avoid blocking executor threads with large cleanup operations.

## Acceptance gates

Keep all existing tests unchanged and add tests for:

- synchronous/async byte-for-byte output equivalence;
- file and non-seekable ingestion;
- deduplication, empty assets, metadata, and sections;
- independent concurrent asset/range reads;
- short reads, short writes, and injected I/O failures;
- malformed indexes and oversized allocation requests;
- destination preservation and failed/interrupted append behavior; and
- default-feature and async-feature builds.

Deliver a minimal web-server integration example, executable public API
examples, reproducible benchmark commands and results, a short explanation of
where blocking occurs, and explicit cancellation/file-mutation limitations.
