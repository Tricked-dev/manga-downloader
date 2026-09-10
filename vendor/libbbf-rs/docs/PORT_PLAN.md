# Rust port plan

The C++ checkout in `libbbf/` remains the behavioral and benchmark reference
until the Rust implementation reaches parity.

## Crate boundaries

- `bbf-format`: BBF v3 constants, fixed-size wire structures, and byte-level
  codecs. This crate has no file I/O.
- `bbf-ffi`: Rust-only C/WASM-compatible reader exports matching the optional
  reference binding and backed by the bounded `bbf-io` reader.
- `libbbf-rs`: public facade and checksum helpers while the API is being
  migrated.
- `bbf-io`: bounds-checked views over owned or borrowed byte storage.
- `bbf-mmap`: safe owned snapshots plus explicitly unsafe zero-copy `bbf-io`
  reader views for inputs guaranteed to remain immutable.
- `bbf-mux`: builder, string pool, deduplication, alignment, and serialization.
- `bbfmux`: Rust CLI and compatibility workflows; read-only modes use owned
  snapshots by default.
- `bbf-bench`: deterministic Rust benchmark driver with operation names aligned
  to the C++ benchmark suite and machine-readable timing output.

## Incremental order

1. Wire-format structures and exact little-endian serialization.
2. String-pool and table validation.
3. Reader views and asset hashing.
4. Builder file layout, alignment, and deduplication. Both the in-memory
   builder and the file-backed `FileBuilder` path are implemented; the latter
   streams unique assets during `add_page` and emits tables/footer at
   `finalize`.
5. Metadata, sections, and petrification.
6. Existing-archive editing and re-encoding. `ArchiveEditor` imports the
   current asset/page/metadata/section tables, supports adding and replacing
   assets, adding pages, changing page references/flags, and emits a valid
   re-encoded archive, including preservation and replacement of opaque
   expansion records.
7. CLI parity and fixture-based compatibility tests. (Core mux, inspection,
   verification, extraction, petrification, file-backed metadata/sections,
   nested section ranges, and numeric-target fixture parity are implemented.)
   The remaining CLI audit is limited to behaviors that are absent or disabled
   in the reference, plus documented reference crash boundaries: `--order`
   has an incomplete unsafe implementation, and footer verification is not
   wired into the reference CLI.
8. Benchmarks and optimization only after parity is measured. (In progress:
   the Rust driver covers the mapped reader and writer cases, streaming paths
   have been measured, the reference Catch2 harness is runnable through Nix,
   exact file-backed 4 KiB and 4 MiB add-page comparisons are recorded, and
   component-level XXH3, buffered-payload-write, asset-table-lookup, and
   page-index-serialization measurements have begun, with the first
   wire-preserving batching optimization applied. Fresh component samples now
   put combined page-index encoding and XXH3 payload hashing ahead of the
   pinned C++ sample, while hash-only page-index work remains near parity;
   neither component currently justifies an async runtime.)

## Delivery phases

Phase 1 is the 1:1 parity gate. It owns the remaining behavior, ABI, wire-byte,
diagnostic, fixture, and reference-bug decisions. Honest code-level speed work
is allowed during this phase when it preserves those observable boundaries and
is measured against the equivalent C++ component. A cache-only speed claim is
not parity evidence, and a cache that the C++ implementation does not have is
recorded as a later tradeoff rather than used to hide a component-level gap.

Phase 2 is the more aggressive optimization road after the parity ledger is
closed and Rust is at least as fast as C++ on each component where practical,
as well as on the end-to-end baseline. It covers optimizations that need a
broader tradeoff review: novel caches, more radical layout or scheduling
changes, and async designs. Each must still be measured across supported
workflows, reported separately from fair C++ parity numbers, and justified by
real benefit. Persistent caches are optional rather than a substitute for
better code; async remains conditional on a measured benefit and a
dependency/scheduling cost review.

Deferred Phase 2 candidates are the per-record page codec gap, broader index
serialization batching, hash implementation/CPU-flag tuning, and any page-byte
cache experiment. These are recorded targets only; they are not part of the
current parity implementation.

The optional reader binding is implemented as `bbf-ffi`; it preserves the
reference symbol names and packed record views while returning null for invalid
or out-of-range handles and entries. Its checked-in
`crates/bbf-ffi/include/bbf_ffi.h` declaration surface is compiled by the Nix
`ffi-header` check, including packed-record size assertions and all exported
function signatures. The pinned Rust toolchain also includes
`wasm32-unknown-unknown`, and the Nix `wasm` check compiles `bbf-ffi` for that
target without introducing a non-Rust runtime dependency.

## Known reference edge case

The reference CLI's zero-initialized extraction union makes bare `--extract`
select asset 0, even when later pages reference other distinct assets. Rust
reproduces this default-path behavior for 1:1 parity; section extraction still
follows each page's `assetIndex`, matching the reference's separate code path
and section-only announcement.

The reference builder also matches only the first four bytes of a file
extension when assigning media types. Rust preserves that behavior, including
compatibility cases such as `.jpegx` being classified as JPEG.

CLI metadata and section parsing likewise preserves backslashes: a backslash
only shields the immediately following colon from field splitting; it is not
removed from the stored value. Only the first two unescaped colons split a
record; any later colons remain in its parent field. File-backed configuration
lines strip leading control whitespace but preserve trailing whitespace in
their fields. Missing `--metafile` and `--sections` files emit the reference
warning and leave muxing enabled; malformed or otherwise unreadable files
still fail safely in Rust.

Every step adds focused Rust tests first, then compares output or behavior
against the C++ reference before the next step begins.

The checked-in [1:1 parity ledger](PARITY_LEDGER.md) tracks confirmed fixes,
reference-only behavior, intentional safety differences, and evidence
blockers. It is the scope boundary for further work.
