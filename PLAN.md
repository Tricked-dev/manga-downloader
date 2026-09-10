# Simplify manga-downloader to a single Rust binary, integrating libbbf-rs + mangajenai-rs

## Context

`manga-downloader-main.zip` is a 9 MB Bazel monorepo: 45.8k LOC of Rust across 20 crates, plus a
SvelteKit SSR frontend, WASM source plugins, a Kotlin Tachiyomi extension, 4.4k LOC of NixOS host
config (Kanidm IdP, Grafana, VictoriaMetrics/Logs/Traces, A/B systemd-sysupdate images), a
TypeScript Grafana-dashboards-as-code generator, k6 stress tests, and a hard dependency on
BuildBuddy remote cache/RBE plus a source-built LLVM toolchain. There is **no Cargo workspace** —
one `third_party/rust/Cargo.toml` of ~85 all-`optional` deps feeds `crates_universe`, and three
separate policy gates actively *forbid* any other `Cargo.toml` from existing.

The product inside all that is good: an Axum API server with SQLite persistence, a download queue,
library/update tracking, a media proxy, and pluggable manga sources. The goal is to keep that
product and throw away the delivery machinery — `cargo build --release` should produce one binary,
and nothing else should be required to run it.

Two sibling repos get folded in at the same time:

- **`../libbbf-rs`** — Rust port of the Bound Book Format (BBF v3): an mmap-able container for
  image-based books with in-file asset/page/section/metadata index tables, XXH3-128 content-hash
  asset dedup, and a per-asset media type. 7 crates, ~7.6k LOC, only two external deps
  (`xxhash-rust`, `memmap2`), no build.rs, no FFI needed. This replaces the server's `.tar.zst`
  archives *and* the ~5k LOC of DB-side archive-index/page-extraction machinery that exists purely
  because tar has no index.
- **`../mangajenai-rs`** — MangaJaNai ESRGAN upscaling via ONNX Runtime. `manga-core` is a plain
  synchronous library (`UpscaleModel`, `upscale_tiled`, model-per-page-height selection from a
  manifest). This becomes a post-download upscale pass that is always compiled in and on by
  default, which makes ONNX Runtime a hard dependency of the binary.

Plus a new **rawkuma** source for Japanese raws, alongside the existing comix source.

## Decisions (locked with the user)

| Area | Decision |
|---|---|
| Build | Plain cargo workspace + crane Nix flake, matching libbbf-rs/mangajenai-rs. Bazel/Aspect/BuildBuddy deleted. |
| Toolchain | Pin the existing `nightly-2026-06-01` in `rust-toolchain.toml`; `.cargo/config.toml` sets `--cfg=tokio_unstable`. No source changes for `Duration::from_mins`, keeps the tokio poll-time histogram. |
| Sources | Native in-process `Source` trait compiled into the binary. wasmtime/WIT/plugin-registry deleted. Port comix, add rawkuma. |
| Storage | `.bbf` is the on-disk chapter format. Native (as-downloaded) pages are the archival copy; upscaled pages live in the same container as a second section. Re-encode only on demand. |
| Resolution | Full resolution everywhere by default. No download-time re-encode, no implicit resize; the existing 800px cap is deleted. `&width=` is opt-in at serve time only. |
| Upscale | **Always enabled** — no cargo feature gate, no opt-out build. ONNX Runtime is a hard dependency and `auto_upscale` defaults to on. |
| Upscale output | Lossless WebP, stored and served as such. WebP is also the default on-demand re-encode target. |
| Dropped | Discord bot; all of `infra/`; codesync / http_proxy / dev-start / `libs/bazel`; k6 + observability TS; kanidm-manager. |
| Kept | OTel telemetry, foyer cache, Cloudflare clearance, OIDC auth, AVIF-on-demand. |
| Auth | OIDC only, verified **in Rust**. Better Auth + `data/auth.db` deleted. |
| Web UI | SvelteKit → `adapter-static`, built with **bun**, embedded in the binary via rust-embed, with an optional load-from-disk override. |
| Clients | **Aidoku + Tachiyomi extensions must keep working** — highest-priority constraint. |
| Rawkuma | Just another source — separate library entries, no EN/JP linking. Browser-driven like comix. |
| Legacy data | None. No `.tar.zst` reader, no migration command; existing downloads are disposable. |

## Target shape

```
manga-downloader/
├── Cargo.toml                 # virtual workspace + [workspace.dependencies]
├── Cargo.lock
├── rust-toolchain.toml        # nightly-2026-06-01
├── .cargo/config.toml         # rustflags = ["--cfg=tokio_unstable"]
├── flake.nix                  # crane; packages.default = manga-server
├── crates/
│   ├── server/                # bin `manga-server` (+ lib manga_server)
│   ├── api-types/  cache/  clearance/  config/  core/
│   ├── fs/  image/  persistence/  runtime/  telemetry/  tls/
│   ├── sources/               # NEW: Source trait, HTTP layer, comix, rawkuma
│   ├── storage/               # NEW: BBF read/write + chapter archive lifecycle
│   └── upscale/               # NEW: mangajanai wrapper (always built)
├── web/                       # bun workspace: SvelteKit SPA + api-client + ui
└── clients/aidoku/  clients/tachiyomi/
```

`cargo build --release` → `target/release/manga-server`, self-contained: sources, web UI,
migrations, and the upscaler all compiled in. It links ONNX Runtime, so that library must be
present to build and to run. Optional runtime inputs: `--web-root` (dev UI override), a models
directory for upscaling, and a CDP endpoint for challenged sources.

## Phase 0 — Import (done)

`github.com/Tricked-dev/manga-downloader` turned out to be **completely empty** — `git ls-remote`
returns nothing, no branches, no commits. So there is no upstream history to branch from and the zip
is the only copy of the code. `main` now exists with this plan as its initial commit (`c293e11`).

Next: commit the extracted zip tree as-is, unmodified, as a second commit. That costs one throwaway
commit but makes every later phase a reviewable diff against a known-good baseline instead of an
unreadable from-scratch drop — which matters most for Phase 3, where ~4k LOC of comix logic moves
between crates and needs to be verifiably unchanged.

## Phase 1 — Cargo workspace

1. Delete every `BUILD.bazel` (51), `MODULE.bazel`, `MODULE.bazel.lock` (1.5 MB), `REPO.bazel`,
   `.bazelrc`, `.bazelversion`, `.bazelignore`, `tools/bazelrc/`, `.aspect/`, `buildbuddy.yaml`,
   `third_party/patches/`, `libs/bazel/`, `start.nu`, `scripts/`.
2. Convert `third_party/rust/Cargo.toml` into the root `[workspace.dependencies]` table — the
   version+feature pins are already correct, just strip `optional = true`. Keep
   `third_party/rust/Cargo.lock` as the starting resolution (`cp` to the root).
3. Author one `Cargo.toml` per crate. Each BUILD file's `deps` list is already explicit and clean:
   `//libs/rust/foo` → `backend-foo = { path = "../foo" }`, `@backend_crates//:bar` → `bar.workspace = true`.
   Keep the existing `crate_name` values (`backend_core`, `manga_server`, …) via `[lib] name`.
4. Move `libs/rust/*` → `crates/*`, `apps/rust/server` → `crates/server`.
5. Delete the three anti-Cargo policy gates (the `flake.nix` `cargo-manifest-policy` check,
   `libs/bazel/cargo:cargo_manifest_policy`, `:cargo_tool_policy`) — they fail by design now.
6. Delete the checked-in `crates/server/src/shadow.rs` stub and let `build.rs`'s `shadow_rs`
   actually run — under cargo it produces real git metadata instead of `"unknown"`/`"bazel"`.
7. Drop `crates/server/src/bin/openapi_export.rs`; the `openapi export` subcommand in
   `crates/server/src/cli.rs` already does the same thing and is what the frontend codegen should call.
8. Native C deps become ordinary build scripts (`zlib`, `lz4`, `zstd`, `mimalloc`,
   `libsqlite3-sys` bundled). No action beyond letting cargo do it.

**Checkpoint:** `cargo build --workspace` succeeds against the *unmodified* source before any
pruning. Do not start Phase 2 until this passes — it isolates build-system breakage from
functional breakage.

## Phase 2 — Prune

Delete outright:

- `libs/rust/discord/` (2,057 LOC) + the four `twilight-*` deps. Remove `Option<DiscordBot>` from
  `AppState` (`crates/server/src/server/state.rs`), the `discord_*` settings from
  `crates/core/src/settings.rs`, and notification call sites in the downloader.
- `infra/` entirely — `infra/nixos` (28 `.nix`, 4,386 LOC: vps-83190 host, Kanidm, Grafana,
  Victoria*, alloy, sysupdate A/B images, sops secrets) and `infra/images/rbe`.
  **Keep** the socket-activation code in `crates/server/src/server/startup.rs` — it's 20 lines,
  harmless without systemd, and useful if you ever redeploy.
- `apps/rust/codesync/` (3,373), `apps/rust/http_proxy/` (450), `apps/rust/dev-start/` (573).
- `observability/` and `stress-testing/` (both separate node workspaces).
- `apps/svelte/kanidm-manager/`.
- `plugins/` and the wasm half of `libs/rust/plugin-host/` — see Phase 3 for the split.
- `libs/rust/page-extraction/` (893) and `crates/server/src/archive_index.rs` (2,251 with its app
  modules) — see Phase 4.

## Phase 3 — Native sources

The WIT world (`libs/rust/plugin-host/wit/manga-source.wit`) is already a clean 5-function
contract. Transcribe it to a Rust trait in `crates/sources/src/lib.rs`:

```rust
#[async_trait]
pub trait Source: Send + Sync {
    fn metadata(&self) -> &SourceMetadata;
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, SourceError>;
    async fn manga(&self, id: &str) -> Result<Manga, SourceError>;
    async fn chapters(&self, id: &str) -> Result<Vec<Chapter>, SourceError>;
    async fn pages(&self, id: &str) -> Result<Vec<Page>, SourceError>;
}
```

The record types (`Manga`, `Chapter`, `Page`, `MediaRef`, `SearchQuery`, `SourceMetadata`,
`SourceCapability`, `SearchOptions`) port over 1:1 from the WIT — kebab-case to snake_case, `option<T>`
to `Option<T>`. `SourceError` keeps the `{ code, message, retryable }` shape so existing API error
mapping and retry logic are untouched.

**What survives from `plugin-host` (~1.65k of 3.96k LOC), moved into `crates/sources`:**

| File | LOC | Fate |
|---|---|---|
| `src/fetch.rs` | 1,275 | **Keep as-is.** This is the real HTTP layer: tower stack with `ConcurrencyLimitLayer` + `CoalesceLayer` + `RetryLayer`, request profiles, clearance integration, media fetch. The two host imports (`fetch`, `capture-browser-json`) become direct method calls for native sources. |
| `src/media.rs` | 310 | **Keep.** `MediaRefSpec` / `FetchRequestSpec` / `MediaTransformSpec` (incl. the comix 5×5 descramble). |
| `src/manager/media_client.rs` | 56 | **Keep** as `SourceMediaClient`. |
| `src/manager/mod.rs` | 1,291 | **Shrink hard.** Keep `SourceInfo`, enable/disable, per-source settings; drop install/upload/artifact-history/live-reload. Becomes a static registry over `Vec<Arc<dyn Source>>`. |
| `src/host.rs`, `src/runtime.rs`, `manager/wasmtime_cache.rs`, `manager/plugin_file.rs`, `manager/metadata_cache.rs` | 356 | **Delete** — wasmtime plumbing. |
| `src/registry.rs` | 651 | **Delete** — remote plugin registry. |

Drops `wasmtime`, `wasmtime-wasi`, `wit-bindgen` and the `.wasmtime-cache` dir.

**Port comix** (`plugins/comix/src/` → `crates/sources/src/comix/`, ~4k LOC): `source.rs`,
`chapters.rs`, `parse.rs`, `models.rs`, `ids.rs` are pure logic and move unchanged. Only `http.rs`
and `browser_capture.rs` change — the `wit_bindgen` imports become `SourceHttpClient` /
`ClearanceSolver` calls. Its inline tests (extensive in `parse.rs` and `source.rs`) come along and
are the acceptance criterion for the port.

**Add rawkuma** (`crates/sources/src/rawkuma/`), base `https://rawkuma.net`. **It goes through the
browser, like comix** — document fetches use the `ClearanceSolver` / browser-capture path rather
than plain `reqwest`, so it inherits challenge solving and real-browser headers. Two consequences:
the browser is now load-bearing for *every* compiled-in source rather than an edge case (see Risks),
and the capture request shape from the old WIT world (`init-script`, `done-expression`,
`payloads-expression`, timeouts) is still the right interface — keep it as the native
`BrowserJsonCaptureRequest` in `crates/sources`, don't design a new one.

It's an HTML site rather than a JSON API, so this needs an HTML parser — **there is none in the
current dep set**. Add `scraper` (html5ever + selectors) to `[workspace.dependencies]`. Implement:
search (`/?s=` and `/manga/?order=popular` for the popular sort), details
(title/cover/author/genres/status/alt-titles from the info table), chapter list (`#chapterlist`
entries → number + published date), and page list (the `ts_reader.run({...})` JSON blob embedded in
the chapter page — grab it with a `payloads-expression` capture and parse with `serde_json` rather
than scraping `<img>` tags, which is both more stable and avoids re-parsing the DOM in Rust).
Declare `SourceCapability::{Search, MangaDetails, ChapterList, PageList}` and set a `Referer`
request profile — rawkuma hotlink-blocks its CDN, and image fetches stay on the plain HTTP client
with that profile rather than going through the browser.

Keep the parsers pure functions over `&str` so they unit-test against checked-in HTML fixtures with
no browser involved, mirroring how `plugins/comix/src/parse.rs` tests today.

**CLI/API:** `plugins list` → `sources list`; `/v1/sources` reports the compiled-in set. Keep the
per-source settings and enable/disable endpoints in `api/routes/sources.rs` — the UI and the DB
`settings` rows already work that way. Drop plugin upload/registry endpoints. Migration `0010`
drops the plugin-artifact tables from `0003_plugin_api_v5_clean_break`.

## Phase 4 — BBF storage

Wire `libbbf-rs` as a git dependency pinned by rev (both repos are yours and neither is on
crates.io); add a `[patch]` block so `../libbbf-rs` can be used for local iteration, and add both as
flake inputs.

New `crates/storage`:

- **Write** (`spawn_blocking`, `bbf_mux::FileBuilder` streams assets as pages are added):
  one `.bbf` per chapter at `<download_path>/<source>/<manga>/<chapter>.bbf`. Native pages go in
  section `"original"`; `ComicInfo.xml` and the cover go in the metadata table (BBF's key/value
  records with parent offsets replace `comicinfo.rs`'s XML-in-tar approach — keep emitting the XML
  string too, for CBZ export compatibility). Media type per asset from `MediaType::{Jpg, Png, Webp,
  Avif, Gif}`.
- **Read** (`bbf_mmap::MappedFile` → `IndexedReader`): page count, page → asset, asset bytes,
  media type. Zero-copy, no extraction, no cache warming.

This is what lets the following be **deleted**:

- `libs/rust/page-extraction/` (893) — the extraction scheduler exists only because tar has no index.
- `crates/server/src/archive_index.rs` (2,251), `app/archive_index_maintenance.rs` (9,920 bytes),
  `app/downloaded_archive_derived_state.rs`, and the `downloaded_archive_index` tables from
  migration `0007`. BBF's footer + index tables are the index, in the file, verified by XXH3.
- `app/chapter_pages/downloaded_reader.rs` shrinks to a thin `IndexedReader` wrapper.
- The `apalis` `archive_index_rebuild` job (`crates/server/src/jobs.rs`) and migration `0009`'s
  queue table — unless Phase 5's upscale job reuses them, which it should.

Keep `libs/rust/image`'s AVIF/JPEG/resize helpers and the comix descrambler; drop
`build_zstd_folder_from_paths` / `reencode_zstd_folder_to_avif` / `inspect_archive` and the tar+zstd
path once nothing calls them.

**Format policy** (this is the answer to "what gets served to the extensions"):

- Downloads store **native bytes, untouched** — whatever the source sent (JPEG/WebP/PNG).
- **No resolution cap anywhere, at any stage.** The per-source destructive `avif_enabled` download
  step and its hardcoded `DEFAULT_AVIF_TARGET_WIDTH = 800` are **removed**. That 800px cap is worse
  than a serving choice — it ran during download, so it permanently discarded resolution from the
  archive. It's also plainly wrong for the actual target: a modern phone screen is ≥1200px wide, so
  even the intended use case was being under-served. Grep `DEFAULT_AVIF_TARGET_WIDTH` and
  `convert_to_avif(` (`libs/rust/image/src/lib.rs:30,55` and 5 call sites) — every default path must
  end up full-resolution, and `convert_to_avif`'s implicit-resize signature should be deleted in
  favour of the explicit `_with_max_width` variant so no future call silently downscales.
- Page reads serve full-resolution stored bytes with the media type from the BBF asset record.
  **The default variant is upscaled when that section exists, falling back to original when it
  doesn't** — so a client that passes no `variant` always gets the best copy on hand, and the
  content type follows it (`image/webp` for upscaled, the source's own type for original).
  Extensions get WebP or the original JPEG/WebP → works on every Android/iOS version either way.
- Re-encoding and resizing are serve-time, on demand, opt-in only: `?format=avif|webp|jpeg` +
  `&width=N`, memoized in the existing foyer hybrid cache (`crates/cache`). No `width` parameter
  means no resize. `app/media.rs:466`'s `media_proxy_format_from_query` already implements exactly
  this for the proxy path — extend it to the downloaded-page path instead of writing something new.
- Where a re-encode *is* needed, WebP is the default target rather than AVIF: lossless WebP is
  small enough on manga line art, decodes everywhere (Android 4.3+/iOS 14+ vs AVIF's Android
  12+/iOS 16+), and matches what the upscale pass writes. AVIF stays available via `?format=avif`.
- **No legacy migration.** `.bbf` is simply the format; there is no `.tar.zst` reader and no
  conversion command. This is a fresh project — any existing downloads are disposable and can be
  re-fetched. Concretely that means the tar+zstd code in `libs/rust/image` is deleted outright
  rather than kept alive behind a read path, which is what makes the Phase 4 deletions clean.

## Phase 5 — Upscaling

`crates/upscale`, unconditionally part of the workspace and linked into the binary — there is no
feature gate, so `ort`/ONNX Runtime is a normal dependency and `pkg-config` must find
`onnxruntime` at build time (`ORT_SKIP_DOWNLOAD=1`, so a missing library is a hard error rather
than a silent download). Wraps `mangajanai-rs`'s `manga-core` (git dep, pinned rev): `ModelManifest::select(height, scale)`
→ `UpscaleModel` → `upscale_tiled` with a `TileConfig`. Inference is blocking and `&mut`, so run it
on a dedicated worker thread with a `ModelCache` keyed by model path — port
`mangajenai-rs/crates/manga-cli/src/convert.rs`'s `ModelCache` rather than reinventing it.

- **Output encoding: lossless WebP.** `image` 0.25's `WebPEncoder` is lossless-only, so the
  existing `image` dep covers it — no libwebp, no new native dep. `manga-core` hands back an
  `image::RgbImage`, so it's a direct encode.
- **Both variants in one container.** Upscaled pages become a second BBF section, `"upscaled"`,
  with its own page list pointing at the WebP assets; `"original"` is untouched. Asset dedup is by
  content hash, so nothing is stored twice. Metadata records `upscale.model`, `upscale.scale`,
  `upscale.tile_size`.
- **Reads:** `?variant=original|upscaled`, defaulting to upscaled when that section exists.
- **Jobs:** reuse the `apalis` SQLite backend freed up in Phase 4 for an `upscale_chapter` job.
  A new `auto_upscale` setting (global, overridable per source using the existing
  `source_setting_key` pattern in `app/settings.rs:220`) **defaults to on**, so every completed
  download enqueues one; the job is also triggerable per chapter from the API for re-runs and for
  backfilling chapters downloaded before a model was available.
- **State:** migration `0011` adds `upscaled_at`, `upscale_model`, `upscale_scale` to the
  downloaded-chapter rows so the UI and API can show what has been upscaled and what hasn't.
- Model files are **not** vendored — `--models-dir` (default `./data/models`), documented as a
  separate download; `../_mj_downloads/models_v1` holds the `.pth` weights, which still need the
  Python `export_onnx.py` step from `mangajenai-rs/tools/` to become `.onnx`.
- **Degrade, don't fail, when models are absent.** Since upscaling is always on, a server with no
  models directory is a normal state (fresh install, models not yet exported). The `upscale_chapter`
  job must log once and mark the chapter un-upscaled rather than erroring in a retry loop, and
  startup should warn — never refuse to boot. The `"original"` section is always present, so
  reads keep working untouched.

## Phase 6 — OIDC in Rust, embedded UI

Auth moves out of the frontend. The settings already exist server-side and are currently only read
by the frontend's Better Auth: `auth_enabled`, `auth_oidc_issuer_url`, `auth_oidc_client_id`,
`auth_oidc_client_secret`, `auth_oidc_scopes`, `auth_oidc_provider_id` (`crates/core/src/settings.rs:20-80`).

- Implement discovery + authorization-code-with-PKCE + JWKS-cached ID-token verification in
  `crates/server/src/api/auth.rs` (which today is 2.3k bytes of static bearer check). Needs new
  deps — `openidconnect` or `jsonwebtoken` + a small discovery client.
- Session as an httpOnly cookie; `BACKEND_API_KEY` bearer stays for the extensions and CLI, which
  cannot do an interactive flow.
- Delete `web/src/routes/api/auth/**`, the Better Auth + `better-sqlite3` deps, and `data/auth.db`.

**UI:** `apps/svelte/web` → `web/`, `libs/svelte/{api-client,ui}` → `web/packages/*`. pnpm → **bun**
(`bun.lock`, `bunfig.toml`; `pnpm-workspace.yaml` → `workspaces` in the root `package.json`).
`@sveltejs/adapter-cloudflare` → `adapter-static` with `ssr = false`; delete the SSR API-proxy
routes under `src/routes/api/app/**` and talk to the backend directly (CORS is already
`allow_origin(Any)`). Keep the orval codegen, sourced from `manga-server openapi export`.

Serve it from axum: `rust-embed` over `web/build`, with an SPA fallback route, and a
`--web-root <dir>` / `WEB_ROOT` override that serves from disk instead (`tower-http`'s `fs` feature
is already enabled and unused). Embedding is `#[cfg_attr]`-gated on a `web` feature so
`cargo build --no-default-features` still works without a bun toolchain present.

## Phase 7 — Keep the extensions working

Highest-priority constraint, and the part most at risk.

- `clients/aidoku/` — **fix a bug that exists today**: `app/clients.rs:143` shells out to
  `cargo build --target wasm32-unknown-unknown --release` in a directory that has **no
  `Cargo.toml`**, so the on-demand build path cannot work at all; only `AIDOKU_PACKAGE_PATH` with a
  prebuilt artifact does. Give it a real `Cargo.toml` (excluded from the workspace, own
  `[profile.release]`, `crate-type = ["cdylib"]`) so both paths work. Keep the `.aix` packaging and
  `/v1/clients/aidoku/package`.
- `clients/tachiyomi/` — Kotlin extension (2 files, 654 LOC) + 28 Java stubs + `scripts/build-apk.sh`
  + debug keystore. Untouched by the Rust changes; the script needs to stop assuming Bazel paths.
  Keep `/v1/clients/tachiyomi/package`.
- Both extensions consume the same HTTP API, so the contract to protect is: `/v1/media/image`,
  the page-read endpoints, and their content types. Phase 4's "serve native bytes" policy is
  *more* compatible than today's opt-in AVIF, so this should be a strict improvement — but it must
  be verified on-device, not just by curl.

## Phase 8 — Flake and CI

Model `flake.nix` on `libbbf-rs/flake.nix` (crane + rust-overlay + flake-utils, which you already
use in both sibling repos):

- `packages.default` = `manga-server` (web UI embedded; bun build as a fixed-output derivation),
  with `onnxruntime` in `buildInputs` and the `ORT_*` env vars set the way
  `mangajenai-rs/flake.nix` already does. There is only one package — no upscale variant.
- `checks` = build, `cargoTest`, `cargoClippy --all-targets -- --deny warnings`, `cargoFmt`.
- `devShell` = toolchain + rust-analyzer + bun + chromium (for clearance) + `onnxruntime` +
  `bbfmux` (for inspecting produced archives). Reuse `mangajenai-rs`'s `python-env.nix` for the
  one-time `.pth` → `.onnx` export.
- Inputs: `libbbf-rs`, `mangajenai-rs`.
- Neither sibling repo has CI today; a single GitHub Actions job running `nix flake check` is worth
  adding here.

## Verification

Build and unit level:

1. `cargo build --workspace && cargo test --workspace` — comix's ported `parse.rs`/`source.rs`
   tests and rawkuma's new fixture tests are the source-port acceptance gate.
2. `nix flake check` and `nix build`.
3. `ldd target/release/manga-server` — confirm ONNX Runtime **is** linked, and that a bare
   `cargo build` outside the dev shell fails with a clear pkg-config error rather than trying to
   download a runtime.

Functional, end to end:

4. `manga-server db migrate` on a fresh dir; `manga-server config print --json`;
   `manga-server sources list` shows comix **and** rawkuma.
5. `manga-server serve`, then via `/v1`: search on both sources, add one of each to the library,
   queue a download.
6. **Cross-validate the container with its own reference tooling** — run
   `bbfmux <chapter>.bbf --info --counts --sections --hashes` and `bbfmux <chapter>.bbf --verify`
   on the produced file. This is a strong independent check that the writer is correct, since
   `bbfmux` is byte-parity-tested against the C++ implementation.
7. `curl -i .../page/0` with no `variant` → the upscaled WebP when that section exists, otherwise
   the original `image/jpeg`/`image/webp`. In the not-yet-upscaled case its pixel dimensions match
   the source image exactly (assert this — it's the regression guard against the 800px cap coming
   back); in the upscaled case they match source × model scale. Then `?format=webp` → `image/webp`,
   still full resolution, `X-Image-Cache: MISS` then `HIT`; `?format=avif` → `image/avif`;
   `&width=1200` → resized only when asked.
8. Trigger the `upscale_chapter` job (or just download a chapter, since `auto_upscale` is on),
   then `bbfmux --info --sections` shows both
   `original` and `upscaled`; `?variant=original` and `?variant=upscaled` return different bytes
   with `image/jpeg` and `image/webp` respectively; the chapter row reports `upscale_model`.
9. Start with an **empty models directory** and download a chapter: the job degrades with one log
   line, the chapter is readable, and nothing retry-loops. Then add models and re-run the job.
10. `curl localhost:4000/` returns the embedded SPA; `--web-root ./web/build` serves from disk;
    OIDC login completes against a real issuer and the session cookie authorizes `/v1` calls.
11. **On-device:** build and sideload both extension packages from `/v1/clients/*/package`, then
    browse → open a manga → read a downloaded chapter in Mihon and in Aidoku. This gate is
    non-negotiable per the constraint above.

## Risks and open items

- **Both sources depend on headless-browser capture**, so `crates/clearance` (chromiumoxide /
  Browser Use / any CDP endpoint) is a hard runtime requirement for normal operation, not an
  optional extra for challenged sites. A build with no reachable browser can serve the library but
  cannot search or fetch new chapters at all. That makes the clearance path worth its own health
  check in `app/health.rs` and a startup warning, and it means chromium belongs in the dev shell.
  Test both sources against a challenged page, not just their happy paths.
- **rawkuma parsers are scraping-fragile.** Fixture-based tests localize breakage but won't prevent
  it; site changes will need parser updates.
- **New dep: `scraper`** (html5ever). First HTML parser in the tree.
- **Nightly + `tokio_unstable`** are load-bearing (`Duration::from_mins` in ~10 places, tokio
  poll-time histogram). Documented in `rust-toolchain.toml`; unpinning is a follow-up, not part of this.
- **ONNX Runtime is now a build-time requirement**, so `cargo build` no longer works on a bare
  checkout — it needs the dev shell or a system `onnxruntime`. That is the cost of dropping the
  feature gate; re-adding it later is a small change if it becomes annoying for contributors.
- **Upscaling every download is expensive.** ESRGAN inference on CPU is minutes per page, so with
  `auto_upscale` on by default the job queue becomes the bottleneck for large libraries and
  storage roughly doubles (both sections retained). Bound the job concurrency to 1 and measure a
  full chapter before turning it loose on a backfill; GPU execution providers are a follow-up
  (`mangajenai-rs` PROGRESS.md has them scheduled).
- **BBF stores payloads uncompressed**, so the `.tar.zst` wrapper goes away. Near-neutral for size
  (already-encoded images barely compress) but worth measuring on a real library before committing
  to the migration.
- **BBF's API is sync + mmap.** Every read/write must go through `spawn_blocking`; a stray sync call
  in an async task will stall the runtime.
- **`README.md` is already stale** (claims a `nixosModules.manga-server` the flake no longer
  exports). It needs a full rewrite for the new build, not a patch.
- **`CONTEXT.md` stays** — the domain glossary (Downloaded Archive vs Archive Index, Route Snapshot
  Invalidation, Stats Projection) is what makes `app/`'s 20 modules navigable, and Phase 4 changes
  several of those terms' meanings. Update it in the same commits.
