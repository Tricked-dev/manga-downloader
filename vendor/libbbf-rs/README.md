# libbbf-rs

Rust rewrite of the Bound Book Format library.

The [`libbbf/`](libbbf/) directory is the original C++ implementation and is
kept as a reference and benchmark target while the Rust implementation is
developed at the repository root.

## Nix

The flake builds both implementations:

```bash
nix develop
nix flake check
nix build                 # Rust implementation
nix build .#libbbf-cpp    # C++ reference implementation
nix run .#bbfmux -- --help # Rust CLI
nix run .#bbf-bench -- reader-verify --input=FILE --iterations=1000
```

The Rust crate is independent of the C++ code and uses Rust dependencies only.
The read-only CLI modes use safe owned file snapshots; `bbf-mmap` also
provides an explicit unsafe zero-copy view for callers that can guarantee
immutability. `bbf-mux::FileBuilder` provides the file-backed streaming
builder, and `libbbf/` remains the pinned behavioral and benchmark reference.
The root `libbbf` facade remains Tokio-free by default; enable its optional
`tokio` feature to access the `bbf_tokio` API through the facade.

`MappedFile::open` is an explicitly unsafe zero-copy API: callers must ensure
the source file is not written or truncated for the mapping's lifetime. Use
`OwnedFile::open` when another process may modify the source concurrently.
Similarly, `Builder::petrify_file` uses destination-local staging; the CLI's
compatibility-only path retains the reference `petrified.bbf.tmp` behavior.

The optional C/WASM reader ABI is provided by `bbf-ffi`; its declarations are
in [`crates/bbf-ffi/include/bbf_ffi.h`](crates/bbf-ffi/include/bbf_ffi.h).
The Nix `ffi-header` check compiles a probe against the header and verifies the
packed record sizes.

## Editing existing archives

`bbf-mux::ArchiveEditor` (also re-exported by the root `libbbf` crate) opens an
existing archive, preserves its asset/page and metadata/section records, and
re-encodes an edited copy:

```rust
use libbbf::{ArchiveEditor, MediaType};

let mut archive = ArchiveEditor::open("book.bbf")?;
let asset = archive.add_asset_bytes(b"new page", MediaType::Png.as_u8(), 0);
archive.add_page(asset, 0)?;
archive.replace_page_asset(0, asset)?;
archive.set_page_flags(0, 1)?;
archive.write_to("edited-book.bbf")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Asset replacement keeps the asset index stable, so all pages that reference it
see the new payload. For normal archives, unchanged table records and the raw
string pool are preserved byte-for-byte; edits append through the same pool.
Petrified or nonstandard physical layouts may be normalized during re-encoding.
Expansion records are preserved and can be replaced or appended. Pages can
also be removed; section boundaries are rebased to the remaining page
sequence. The builder and sealed-file appender also expose byte-oriented
metadata and section methods when string-pool data is not UTF-8.

The async benchmark matrix is reproducible with the pinned toolchain:

```bash
TMPDIR=$PWD/.tmp nix develop -c cargo run -p bbf-tokio --release \
  --example matrix -- 3
```

The minimal bounded-memory server example serves asset 0 without buffering
the payload in the handler:

```bash
nix develop -c cargo run -p bbf-tokio --example serve -- archive.bbf
```

For append-only edits that must retain existing payloads in place, use
`bbf_mux::FileAppender` (or `bbf_tokio::AsyncFileAppender` with Tokio). It
loads only the index and string pool, appends new payloads and a replacement
index/footer, and makes the new view visible on `finalize`; dropping it leaves
the previous footer authoritative. `AsyncArchive::petrify_to` provides the
bounded streaming petrification transform for web-server workflows. For large
asset responses, `AsyncArchive::asset_reader` and
`AsyncArchive::asset_reader_range` provide bounded `AsyncRead` views without
allocating the payload; each view uses positional reads on the archive's
retained file handle and can be consumed directly by a web server. Both
appender surfaces can also remove pages,
metadata, and sections by appending a replacement index while retaining the
existing payload ranges.
