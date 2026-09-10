# Manga Downloader

A self-hosted manga library with native Comix and Rawkuma sources, chapter downloads,
a browser reader, and Aidoku/Mihon extensions. One Rust server serves the API, static
Svelte application, authentication and installable reader packages.

Downloads preserve the original page bytes and dimensions in a BBF container. Background
upscaling appends lossless AVIF pages to the same file; originals remain readable throughout.
SQLite is the default database, with PostgreSQL supported for both application data and jobs.

## Build and run

Install Nix with Flakes and [devenv](https://devenv.sh/getting-started/). BBF and the upscaler
are vendored in this repository; builds need no private sibling repositories or access tokens.
Model weights are separate and are not required to start the server.

```sh
nix build
./result/bin/manga-server serve
```

The release package embeds the web app and both reader extensions. Open
[the web application](http://localhost:4000). Both sources require a reachable browser to
search or fetch new chapters. To run the development browser on Linux:

```sh
devenv --profile browser up
```

Then start the server from another shell:

```sh
MANGA_SERVER_BROWSER_CDP_URL=http://127.0.0.1:9222 ./result/bin/manga-server serve
```

An existing remote CDP endpoint or Browser Use connection also works. The saved library can
still be read without a browser. Configure authentication before exposing the service beyond
a trusted network; see [authentication](docs/authentication.md).

This is a fresh database boundary. Use a new database and download directory for this version;
the historical schema and archive format are not migrated automatically. See
[database backends](docs/persistence.md).

## Development

`devenv` owns the development shell; the Flake provides packages and checks.

```sh
devenv shell
bun install --frozen-lockfile
bun run build                         # export OpenAPI, generate client, build static UI
cargo run -p manga-server -- serve
```

For frontend development, run `bun run dev` alongside the backend. Vite proxies `/v1` and
`/auth` to port 4000. `PUBLIC_URL` must match the origin used for OIDC login; the registered
callback is `/auth/callback`. Build without embedded web files with
`cargo build -p manga-server --no-default-features`, and serve a disk build with
`--web-root ./web/build` when needed.

The Nix web build pins its output hash in `nix/packages.nix`. After changing the frontend or
generated API, set `outputHash` to `lib.fakeHash`, run `nix build`, and replace it with the hash
reported by Nix. The package name includes an input fingerprint so changed sources cannot silently
reuse an older bundle. Builds use `SOURCE_DATE_EPOCH` for a deterministic frontend timestamp.

```sh
cargo run -p manga-server -- config print --json
cargo run -p manga-server -- sources list
cargo run -p manga-server -- db migrate
cargo run -p manga-server -- openapi export
```

Optional development profiles:

| Profile | Adds |
| --- | --- |
| `browser` | A Linux headless Chromium process on loopback port 9222 |
| `postgres` | A disposable local PostgreSQL service and `test-postgres` command |
| `clients` | Android SDK, JDK and the pinned Kotlin compiler for extension builds |
| `models` | The vendored Python environment for one-time ONNX model export |

### PostgreSQL

```sh
devenv --profile postgres up
# In another terminal:
devenv --profile postgres shell
manga_db_url="postgresql://$USER@$PGHOST:$PGPORT/manga?sslmode=disable"
cargo run -p manga-server -- db migrate --database-url "$manga_db_url"
cargo run -p manga-server -- serve --database-url "$manga_db_url"
```

Entering the base shell does not set `DATABASE_URL` or start PostgreSQL. A production PostgreSQL
URL should name a dedicated application database and role; grant schema creation for the initial
application and Apalis migrations, and use the server's required TLS configuration. Back up both
the database and the download directory. Test URLs must point to an isolated cluster because the
integration suite creates temporary databases.

Development services require their configured ports to be free. PostgreSQL uses 55432 by
default; `test-postgres` verifies the connected server's data directory before creating test databases.

## Configuration

Bootstrap options accept CLI arguments as well as environment variables. Supported nonempty
settings environment variables initialize or override persisted settings at startup. The web
Settings page controls library updates, download behavior, source enablement and automatic upscaling.

| Environment variable | Default / purpose |
| --- | --- |
| `DATABASE_URL` | `./data/manga.db`; SQLite path or PostgreSQL URL |
| `SERVER_ADDR` | `0.0.0.0:4000` |
| `DOWNLOAD_PATH` | `./data/downloads` |
| `CACHE_DISK_PATH` | `./data/cache` |
| `MODELS_DIR` | `./data/models`, containing `models.json` and ONNX files |
| `UPSCALE_DEVICE` | `cpu` on Linux, `coreml` on macOS; explicit accelerators never fall back to CPU |
| `WEB_ROOT` | Optional static web directory override |
| `PUBLIC_URL` | Fixed external origin, required for OIDC |
| `BACKEND_API_KEY` | Bearer credential for extensions and API clients |
| `AUTH_ENABLED` | `false`; enable with OIDC settings from the authentication guide |
| `MANGA_SERVER_BROWSER_CDP_URL` | Browser CDP endpoint; takes precedence over Browser Use |
| `BROWSER_USE_API_KEY` | Optional Browser Use credential |
| `AIDOKU_PACKAGE_PATH` | Optional prebuilt `.aix` override |
| `TACHIYOMI_PACKAGE_PATH` | Optional prebuilt APK override |

Automatic upscaling defaults to enabled globally and per source. The global switch controls all
sources; a source can opt out separately. Default jobs use 2× models and one job at a time. Intel
CPU inference uses two threads. Chapters with a median original page width of at least 2000 pixels
are left unchanged; 1400-pixel originals are still upscaled, with no cap on the model's output width.
Missing models cause one terminal skip rather than a retry loop. Add the model files and queue the
chapter again to retry it. See [models and upscaling](docs/upscaling.md).

## API and readers

The generated API specification is available at `/openapi.json`. The browser talks directly to
Rust under `/v1`; browser sessions use HTTP-only cookies, and reader extensions use bearer auth.

Downloaded pages use `/v1/library/chapters/{id}/pages/{page}`. The default is the upscaled variant
when available, otherwise the original. Use `?variant=original` or `?variant=upscaled` explicitly.
`?format=avif|webp|jpeg` converts without resizing; `&width=1200` resizes only when requested.
`X-Image-Cache` reports `MISS`/`HIT` for the conversion cache, and `X-Page-Variant` reports the variant.

Install packages from `/v1/clients/aidoku/package` and `/v1/clients/tachiyomi/package`. On-device
setup and build instructions are in [Aidoku](clients/aidoku/README.md) and
[Mihon](clients/tachiyomi/README.md). Device URLs must be reachable from the reader.

## Validation

```sh
nix flake check
# Inside devenv shell:
check-rust
bun run check
bun run test
# With the PostgreSQL profile running:
test-postgres
```

The browser integration suite uses a real Dex issuer and Rust server; see
[its instructions](web/e2e/README.md). Inspect a downloaded container with the vendored BBF tool:

```sh
bbfmux chapter.bbf --info --counts --sections --hashes
bbfmux chapter.bbf --verify
```

The completed checks and measured CPU performance are recorded in [migration verification](docs/verification.md).

## Layout

- `crates/`: Rust server, native sources, persistence, storage, images, upscaling and shared infrastructure.
- `web/`: static Svelte application and its API client/UI packages.
- `clients/`: standalone Aidoku and Mihon extensions.
- `vendor/`: pinned BBF/upscaler source and the SQLx compatibility patches.
- `nix/`: release and tool packages; `devenv.nix` configures development.
- [CONTEXT-MAP.md](CONTEXT-MAP.md): domain documentation for each boundary.

Source behavior is compiled into the server. See [adding a source](docs/backend-sources.md).
