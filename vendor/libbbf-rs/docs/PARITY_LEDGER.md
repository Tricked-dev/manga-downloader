# 1:1 Parity Ledger

This ledger is the scope guard for the Rust rewrite. An item belongs here only
when it affects observable behavior, public API, wire bytes, diagnostics, or
the evidence needed to compare the Rust implementation with `libbbf/`.

Performance work is allowed only after the corresponding behavior is covered by
tests or a documented reference comparison. Improvements that intentionally
change reference behavior are recorded as such instead of being silently
treated as parity.

## Status vocabulary

- **fixed** — Rust behavior is covered and matches the reference evidence.
- **tracked** — observed gap or question; no behavior change is implied.
- **intentional difference** — Rust is safer or more correct than the reference;
  compatibility must be decided before changing it.
- **blocked** — evidence is unavailable because the reference or its tooling
  cannot currently provide a reliable result.
- **no-op parity** — the reference accepts an input but has no effective
  behavior for it, so Rust intentionally keeps the same surface behavior.

## Fixed parity items

- BBF v3 wire constants, fixed-size records, little-endian codecs, alignment,
  string pooling, asset deduplication, and table/footer layout.
- In-memory and streaming file-backed builders, including unique-asset
  streaming, final index/footer emission, and petrification behavior.
- Metadata string-pool insertion order matches the reference: key, value, then
  optional parent, preserving byte-for-byte output and footer hashes for
  parented metadata.
- In-memory builders, file appenders, and async writer/appender surfaces expose
  byte-oriented metadata and section variants, preserving non-UTF-8 string-pool
  bytes while retaining the `&str` convenience methods.
- Sync and async sealed-file appenders expose raw string-pool entries alongside
  their UTF-8 convenience accessors, so existing non-UTF-8 metadata remains
  inspectable before an edit is finalized.
- Sealed-file appenders can remove pages, metadata, and sections by appending a
  replacement index without copying existing payload ranges; page removal
  rebases section starts like the in-memory editor, and both sync and async
  paths have regression coverage.
- Builder configurations carrying `BBF_PETRIFICATION_FLAG` preserve the flag
  but still finalize in the reference's normal layout; the footer remains at
  the end rather than moving immediately after the header.
- Reader bounds checks, mapped reads, asset hashing, verification, extraction,
  nested section ranges, numeric targets, and CLI fixture coverage.
- The byte-backed reader has a deterministic malformed-input regression corpus:
  bounded arbitrary byte sequences exercise header, footer, table, string, and
  hash views without panicking; malformed inputs remain reported as bounded
  reader errors or absent entries.
- Streaming copy/hash paths explicitly cover partial writes and injected write
  failures; async non-seekable ingestion covers short reads, injected reader
  failures, recovery without publishing partial input, and final output
  integrity.
- Async writer abort explicitly removes unpublished staging without creating or
  replacing the requested destination; sealed-file appender abort continues to
  leave its previous footer authoritative.
- Reader string access scans to the bounded string-pool end rather than using a
  fixed 2,048-byte limit, so long ComicInfo.xml metadata remains readable.
  The bounded pool scan still requires a NUL terminator and cannot read past
  the archive's declared string-pool range.
- Media detection uses the reference's first-four-extension-byte rule,
  including compatibility cases such as `.jpegx` and `.avifx`.
- CLI metadata and section parsing preserves reference backslashes, shields
  only the immediately following colon, splits only the first two unescaped
  colons, preserves trailing field whitespace, and distinguishes an explicitly
  empty parent field from an omitted parent.
- File-backed metadata and section parsing matches the reference's additional
  delimiter behavior: an unescaped colon after the parent terminates that
  field, while an escaped colon remains part of the raw parent bytes.
- Unresolved file-backed section targets preserve raw non-UTF-8 bytes in the
  warning written to stdout, matching the reference's byte-oriented `printf`
  path rather than rendering replacement characters.
- Missing or unreadable `--metafile` and `--sections` paths warn and continue
  muxing like the reference; malformed records remain permissive no-ops or
  fallback entries as documented below.
- Inline and file-backed metadata and section configuration honor the
  reference's `MAX_ENTRIES` capacity of 256, silently ignoring later records
  while preserving the earlier records and resulting wire bytes.
- Section filename targets reproduce the reference resolver's current bound:
  it examines only the first `sectionCount` files, warns when a later filename
  is not found, and falls back to page zero. The bound is clamped safely at the
  Rust slice length instead of permitting the reference's possible out-of-range
  access. When a file-backed sections list is longer than the input file list,
  the reference can dereference past that list and terminate with `SIGSEGV`;
  Rust preserves the warning/success behavior without reproducing the crash.
- Verify-mode `--asset=N` reproduces the reference parser's union-member bug:
  the option is accepted but whole-file verification still runs. Asset zero is
  selected only in extraction mode, matching the reference's mode-specific
  behavior.
- Verify-mode `--rangekey=VALUE` reproduces the reference extraction/verify
  union overlap: the pointer-sized range-key value is interpreted as an
  invalid asset index instead of being ignored. The address-dependent number
  is checked by shape and status in the compatibility regression.
- Empty or unreadable-input-directory CLI muxing preserves the reference's
  blank-header output, success status, `[BBFCODEC] No assets to finalize.`
  diagnostic, and `Muxed 0 files` report; direct builder finalization still
  reports `NoAssets` to callers.
- A regular input page that cannot be opened is reported as
  `[BBFCODEC] Unable to open PATH for reading.` on stderr, skipped, and counted
  in the successful mux summary like the reference; direct builder APIs still
  return the underlying I/O error.
- The safe library `Builder::petrify_file` stages beside the destination in a
  unique create-new file, removes staging after transform or rename failures,
  and commits with rename only after a complete transform. The compatibility
  CLI explicitly uses `Builder::petrify_file_compat`, which retains the
  reference working-directory `petrified.bbf.tmp` and its rename-failure
  artifact; a focused regression test covers that boundary.
- Petrification CLI status announcements match the reference: the start line
  includes the input/output paths, followed by `Success.` or the failed-status
  line on stdout.
- Missing-input diagnostics for default mux mode and `--info` match the
  reference's status 1, stdout text, and empty stderr contract.
- `--help` matches the pinned reference text, spacing, status 0, and empty
  stderr contract, including the platform delimiter note.
- `--info` sections are opt-in: `--counts` and bare `--info` do not implicitly
  append a metadata section; `--metadata` remains the explicit selector.
- Info `--pages` and `--strings` flags preserve the reference's accepted
  no-op behavior and add no output or side effects.
- When `--petrify=PATH` precedes `--info`, Rust reproduces the reference
  mode-union overlap that enables the metadata section; the later `--info`
  mode does not perform petrification.
- When `--petrify=PATH` precedes `--verify` or `--extract`, Rust reproduces
  the same union pointer as `sectionName`: verify reports the missing section,
  while extract requires a range key. In extract mode, an explicit section
  also takes precedence over `--asset`, matching the reference branch order.
- `--info` on a missing file matches the reference's status 1, stdout header
  failure, and `[BBFCODEC] Unable to open file ...` stderr diagnostic.
- `--info` on a file with a readable header but an unavailable footer reports
  `Unable to retrieve footer.` on stdout with status 1 and empty stderr.
- `--verify --section=NAME` reports a missing section on stdout with status 1,
  matching the reference's exact message and empty stderr contract.
- `--verify --section=NAME` reports a section with no pages on stdout with
  status 1, matching the reference's exact message and empty stderr contract.
- Verification reports a mismatched asset hash as a `[FAIL]` record, continues
  through the remaining pages, and exits successfully like the reference;
  the compatibility test corrupts payload bytes without changing the index.
- Section extraction without `--rangekey` reports the reference's exact
  stdout diagnostic and status 1 with empty stderr.
- Section extraction with a missing section and a rangekey falls back to all
  pages, including the reference's success status and announcement. This
  preserves the reference's currently observable fallback instead of its
  commented-out "section not found" error path.
- Asset extraction announces the destination before writing and reports an
  unwritable destination as `[BBFMUX] Failed to write file: PATH` while
  retaining a successful exit status, matching the reference's non-creating
  `--outdir` behavior.
- Default extraction reproduces the reference's zero-initialized extraction
  union: bare `--extract` selects asset 0, even when later pages reference
  other distinct assets. Section extraction continues to follow each page's
  `assetIndex` and announces only the section-level operation.
- Extraction opens the hashes report before the metadata report and reports a
  report-open failure as `[BBFMUX] Unable to open file: PATH` on stdout with
  status 1 and no stderr.
- Extraction preserves the reference's unchecked report-write boundary: a
  report that opens successfully but later fails during writing does not abort
  extraction or change its successful status. Open failures remain fatal.
- Asset extraction preserves the same unchecked write boundary: an output file
  that opens successfully but cannot accept all payload bytes remains a
  successful extraction with no write-failure diagnostic; output open failures
  retain the reference diagnostic.
- Extraction creates only explicitly requested reports; bare `--write-meta`
  uses the reference's `path.txt` default, while `--write-hashes` uses
  `hashes.txt`.
- Default extraction announcements omit the `./` prefix; an explicit
  `--outdir=.` retains it like the reference.
- `--section=NAME` is consumed only by the reference's active extract/verify
  modes; when parsed under info/petrify it is ignored rather than leaking into
  a later mode.
- The public facade exports both `Builder` and `FileBuilder` with compile-level
  coverage.
- The public facade keeps Tokio out of default builds and exposes the optional
  `bbf-tokio` API through the opt-in `tokio` feature; both feature modes have
  compile/test coverage.
- Async format errors preserve their underlying `DecodeError` through
  `std::error::Error::source`, alongside the existing I/O, builder, append,
  and task causes.
- Synchronous reader, builder, and editor errors preserve nested I/O and format
  causes through `std::error::Error::source`, so the async facade can expose a
  complete underlying error chain.
- The async facade now includes compile-checked rustdoc examples for opening,
  reading, verifying, and staged archive construction; the runtime server and
  benchmark examples remain separate integration examples.
- Async file-backed construction is compared byte-for-byte with the
  synchronous `Builder` for matching payload, page, table, and footer output.
- Async sealed-file appender opening applies the configured index allocation
  limit before allocating its replacement-index snapshot, with synchronous
  `FileAppender::open_with_index_limit` coverage for the shared boundary.
- Async archive and appender opening have a deterministic malformed-input
  corpus covering truncation, footer-offset overflow, string-pool overflow,
  and reversed index ranges; each is bounded to an error without panicking.
- Async positional-read coverage now exercises two independent assets through
  cloned archive handles concurrently, not only two ranges of one payload.
- The optional reader binding exports the reference C/WASM reader symbols,
  packed header/footer/table record views, string and asset-data pointers, and
  XXH3-128 asset hashing through the Rust-only `bbf-ffi` crate. Its ABI test
  covers the complete valid table/entry sequence, null tables, out-of-range
  entries, and matching hashes. The valid sequence uses the same returned
  header/footer/table pointers as the reference; foreign or malformed pointer
  handling remains a documented Rust safety boundary.
- The FFI surface ships a checked-in `bbf_ffi.h` declaration header. The Nix
  `ffi-header` check compiles a C probe with packed-record size assertions and
  references every exported symbol, keeping the C/WASM-facing ABI visible and
  build-tested.
- Every exported unsafe FFI entry point now documents its pointer, lifetime,
  and ownership requirements; the crate no longer suppresses the missing
  safety-documentation lint globally.
- The Rust reader binding preserves the reference footer-cache sequence:
  string and entry access is unavailable until `get_bbf_footer` has been
  called, while table views remain available from a valid footer pointer.
- The raw FFI asset-data view preserves the reference `isSafe(fileOffset)`
  boundary, including a non-null one-past pointer when `fileOffset ==
  fileSize`; callers must not dereference that sentinel. Higher-level Rust
  asset reads remain checked against the asset length.
- The Rust benchmark driver covers the reference's 40 MiB in-memory writer
  cases as `writer-add-40mb` and `writer-dedup-40mb`; these are kept separate
  from file-output timings. The repaired reference Catch2 harness now provides
  stable comparison samples for the next exact file-backed slices.
- The Rust benchmark driver also covers the reference file-backed constructor
  as `writer-constructor`, including creation and close of the blank-header
  output file.
- The Rust benchmark driver now has exact 4 KiB and 4 MiB file-backed add-page
  slices, `writer-add-file-4kb` and `writer-add-file-4mb`, whose timed boundary
  matches the reference builder's constructor, source-file hash/stream, and
  close path. Larger add-page and deduplication comparisons remain separate
  benchmark slices.
- The benchmark driver now also has an exact file-backed duplicate-page slice,
  `writer-dedup-file-4mb`, with the first add outside the timed region and the
  repeated source-file hash/read inside it. Rust measured about 1.77x faster
  than C++ in the fresh matched sample (245.91–248.04 versus 437.58 µs/op),
  without a cache or behavior change. The matching 4 KiB slice is also faster
  in Rust at about 1.15x (7.95–8.11 versus 9.13 µs/op).
- The benchmark driver also isolates XXH3-128 over a deterministic 4 MiB
  payload as `component-hash-4mb`, with a matching checksum-preserving C++
  reference case. Three fresh runs show Rust at about 1.07x the C++ component
  time; no async change is justified by this CPU-bound gap. A generic versus
  `target-cpu=native` Rust comparison measured 97.93 versus 97.98 µs/op, so
  target flags alone are not currently an optimization.
- A fresh matched release sample at `6834a1b` measured Rust faster for the
  4 MiB hash component (105.83 versus 119.42 µs/op C++, 10-sample reference)
  and combined page-index encoding/hash (195.84 versus 232.02 µs/op C++).
  Hash-only page-index work remains near parity at 104.40 versus 100.65 µs/op;
  these are local measurements, not a cache-based claim.
- The benchmark driver also isolates a successful open-addressed asset-table
  lookup as `component-dedup-lookup-4mb`, with the precomputed hash and table
  setup outside the timed region. Rust measured about 1.23x faster than C++ in
  the first matched run; this does not replace the full deduplication benchmark.
- The benchmark driver also isolates 4 MiB of page-index encoding and hashing
  as `component-page-index-4mb`, using the production Rust `Page` codec and
  packed C++ `BBFPage` records with identical field values. Rust is now about
  1.08x slower after the portable batched 16-byte wire write; the change
  reduced the Rust component time from about 309 µs/op to about 175 µs/op
  without changing wire bytes. The remaining small gap points to per-record
  encoding and allocation costs, not async I/O.
- The benchmark driver also exposes
  `component-page-index-hash-4mb`, which hashes the same pre-encoded page
  index and maps to the reference's cached-hash component. It is measurement
  only: no Rust cache is used, and its result is not counted as a speedup over
  the reference.
- The benchmark driver also isolates buffered 4 MiB payload writing as
  `component-write-4mb`, with matching open/write/flush/close boundaries and a
  checksum-preserving C++ reference case. The corrected same-directory run
  measured Rust at about 1.28x faster than C++; this is filesystem noise evidence, not a
  justification for an async runtime or a behavior change.
- The benchmark driver also covers metadata and section insertion with setup
  outside the timed region. Fresh samples are near parity for metadata and
  unparented sections. Inlining the section wrappers and string-pool call
  reduced longer Rust parented-section samples to 21.90–22.38 ns/op versus
  18.54 ns/op in C++ (about 1.18x slower). This is a narrow code-level
  optimization only; it does not authorize caching, async work, or a change to
  the reference behavior.
- Async full verification now hashes each distinct payload through one coarse
  blocking operation with a reusable bounded `BufReader`, rather than issuing
  a positional system read for every chunk. On the 40 MiB fixture this reduced
  the async sample from 14.42 ms/op to 3.95 ms/op; the matching synchronous
  mapped-reader sample is 1.58 ms/op. The remaining gap is recorded for
  investigation and is not presented as parity.
- The async benchmark driver now includes a matched `dedup-file` slice with
  first ingestion outside the timed region. Async measured 275.44 us/op for
  4 MiB versus the synchronous 257.55 us/op baseline, and 4.36 ms/op for
  40 MiB versus 3.83 ms/op; the modest gap is recorded without adding a cache
  or per-chunk task pipeline.
- Reader string access preserves raw non-UTF-8 string-pool bytes through the
  byte-oriented API; info and metadata reports now emit those bytes like the
  reference instead of treating them as corrupt UTF-8.

## Intentional differences requiring a compatibility decision

- Rust rejects invalid reader ranges and indexes safely. The reference has
  permissive or unsafe boundary checks in some direct-reader paths; notably,
  its `isSafe(count, index)` check uses `>` so `index == count` returns a
  one-past-end table pointer instead of null. The Rust FFI keeps the bounded
  behavior until bug-for-bug exposure is explicitly required.
- For a missing file, the C++ `create_bbf_reader` returns a non-null object
  whose mapped buffer is null after printing its open diagnostic; Rust returns
  a null handle immediately. Keep the Rust constructor failure bounded until
  an ABI policy explicitly requires exposing unusable reader objects.
- The reference CLI can dereference undersized/corrupt mapped input before it
  has enough bytes for a header and may terminate with `SIGSEGV`; Rust returns
  a bounded reader error instead. Valid-header/truncated-footer input is
  matched separately by the diagnostic above for `--info`; `--verify` remains
  deliberately bounded and returns a reader error where the reference crashes.
- The C++ footer view accepts non-zero reserved footer bytes and continues
  (`--info --counts` succeeds); Rust rejects the malformed footer and reports
  the bounded footer diagnostic. Preserve the strict decoder until a
  bug-for-bug compatibility policy explicitly requires accepting malformed
  wire data.
- The C++ `checkMagic` helper checks only the four magic bytes, so a header
  with valid magic and an unsupported version can still pass that helper. Rust
  decodes the complete header before exposing a reader view and rejects the
  unsupported version; keep this bounded malformed-header behavior until an
  explicit ABI compatibility decision requires otherwise.
- The C++ `getStringView` compares its pool-relative string offset against the
  absolute pool end. A one-past-relative offset can therefore be accepted and
  scanned beyond the string pool; Rust rejects offsets at or beyond the pool
  size. The Rust regression keeps this bounded behavior until an explicit ABI
  decision requires reproducing the out-of-pool read.
- The pinned C++ CLI exits with `SIGSEGV` for bare info-only flags without an
  info mode, including `--counts`, `--header`, `--footer`, `--offsets`,
  `--metadata`, `--sections`, `--pages`, `--strings`, and `--hashes`, while
  Rust rejects those incomplete invocations with a bounded usage error. Keep
  the bounded Rust behavior until reproducing an option-state crash is an
  explicit compatibility decision.
- The pinned C++ CLI also dereferences a null input for bare `--verify` and
  `--extract` (after printing its `(null)` open diagnostic), and its bare
  `--petrify=OUTPUT` path reports a null-input failure. Rust returns a bounded
  usage error for all three missing-input forms and does not create output.
- A bare or empty `--outdir` is treated by C++ as a non-null empty C string,
  producing an absolute `/page_N.*` destination. Rust keeps the empty path
  relative to the working directory, avoiding an unintended write outside the
  invocation tree; the compatibility test locks this safer behavior until an
  explicit compatibility decision requires reproducing the reference path.
- The pinned C++ mux exits with `SIGSEGV` when `--order=FILE` is supplied:
  its ordering loop is commented out but still leaves the file list/count state
  uninitialized. Rust accepts the reference option as a safe no-op and keeps
  normal directory scanning; do not reproduce this unsafe crash or invent
  ordering semantics until the reference implements the feature.
- The reference's `atoi`-to-unsigned numeric path permits negative and
  overflowing alignment/ream values to reach unsafe shifts or output sizing;
  the pinned negative-alignment probe does not terminate safely. Rust accepts
  the defined decimal-prefix cases but rejects negative nonzero and overflowing
  values, preserving a bounded builder contract.

These are not optimization targets. If strict bug-for-bug compatibility is
required, each needs an explicit test and policy decision first.

## Reference behavior to keep tracked

- The Rust `ArchiveEditor` is an additive editing API rather than a C++ parity
  surface. Normal non-petrified archives now preserve unchanged table records
  and raw string-pool bytes byte-for-byte during re-encode; petrified or
  nonstandard physical layouts may still be normalized. Expansion records are
  now preserved and can be replaced or appended as opaque format records.
- The reference CLI does not wire footer verification into its normal command
  path. Keep any Rust verification behavior separately documented and tested.
- The reference-compatible petrification transform preserves the original
  footer hash while rebasing asset offsets, so the resulting petrified file's
  index checksum does not verify even though its tables and payload hashes are
  readable. The async reader reports `footer_verified = false` for this known
  layout behavior and still verifies each payload.
- File-backed configuration now follows the reference's raw-byte parser,
  including its platform-observable leading signed-byte/control trim, and
  preserves the remaining non-UTF-8 fields through the byte-oriented builder
  path. Command-line arguments remain UTF-8 because they are provided through
  Rust's process argument API.
- Exact CLI diagnostics and exit codes for malformed inline and file-backed
  configuration are covered: missing metadata values are ignored, missing
  section targets fall back to page zero, unresolved targets warn on stdout,
  and muxing still succeeds with the reference output boundary.
- The reference CLI accepts malformed inline/file records and handles them
  permissively: a metadata record without a value is passed to
  `addMeta` and becomes a no-op, while a section without a target resolves to
  page zero. Rust now matches those cases, warns and falls back to page zero
  for an unresolved filename, and ignores an out-of-range section add like the
  reference. Rust also ignores unknown options and accepts the reference's
  bare forms for mux configuration flags; mode-specific `--sections` behavior
  is covered separately. Bare `--petrify` now reports the reference missing
  output error with status 1 instead of treating a positional path as output.
- **intentional safety boundary — malformed file-backed petrification:** Rust
  rejects invalid default-layout ranges and malformed asset records before or
  while staging. A fixture with `assetOffset > footerOffset` returns an error
  before Rust creates its temp file; the C++ reference reaches `copyRange` and
  leaves a 320-byte `petrified.bbf.tmp` after the same operation. A fixture with
  a non-zero reserved asset byte is accepted by C++ and emitted unchanged,
  while Rust's checked `Asset::decode` rejects it and removes the staged file.
  These are tracked reference bugs, but reproducing their permissive or
  failure-side-effect behavior would weaken the Rust safety contract. Valid
  transforms retain the reference's staged-file and rename boundary.

## Benchmark and tooling status

- The C++ Catch2 benchmark is runnable through the pinned Nix package. The
  package applies `patches/libbbf-bbfbench-no-shallow-copies.patch`, which fixes
  benchmark-only shallow-copy returns for `BBFBuilder`/`BBFReader`, adds the
  metadata entry required by the reader setup, corrects the reader hash asset
  index, and preserves a scalar checksum for the XXH3 component case. The full
  `bbfbench "Performance Benchmarks" -r compact` filter now passes; its samples
  are ready for the next speed-parity slice.
- The configured `wasm32-unknown-unknown` target is now an explicit Nix check;
  `bbf-ffi` compiles for it using the Rust implementation and checked-in ABI
  types.

## Working rule

Before implementing the next slice, move the relevant tracked item to fixed,
intentional difference, or blocked and add the smallest test or reference
evidence that justifies the transition. Keep optimization changes separate
from parity changes so regressions remain attributable.
