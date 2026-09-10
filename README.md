# Manga Server

Self-hosted manga downloader with a Rust backend, a SvelteKit web UI, and WebAssembly source plugins.

Use it to search configured manga sources, keep a local library, track new chapters, queue downloads, export `.tar.zst` archives, and read chapters in the browser. The server owns source plugins, media proxying, update jobs, optional OIDC auth, Discord notifications, and Cloudflare challenge clearance through Browser Use or a remote CDP browser.

## Features

- Axum backend with SQLite/Turso-backed persistence and background jobs
- Wasmtime source plugin host using `libs/rust/plugin-host/wit/manga-source.wit`
- SvelteKit app for library, updates, sources, downloads, clients, settings, and status
- Runtime image/media proxying with AVIF conversion support
- Plugin upload, enable/disable, settings, artifact history, live reload, and bundled source support
- Browser Use or remote CDP clearance for sources that need a browser
- Optional Better Auth OIDC login
- OpenTelemetry traces, logs, and metrics over OTLP
- Aidoku client package served from the backend
- Cloudflare Workers support for the frontend

## Repository Layout

```text
.
├── apps/
│   ├── rust/server/          # Axum API server
│   ├── rust/dev-start/       # Bazel-built dev launcher
│   └── svelte/web/           # SvelteKit web app
├── libs/
│   ├── rust/core/            # shared domain types
│   ├── rust/clearance/       # embedded browser clearance
│   ├── rust/fs/              # filesystem helpers
│   ├── rust/image/           # image processing
│   ├── rust/persistence/     # database and settings
│   ├── rust/plugin-host/     # Wasm plugin host
│   ├── svelte/api-client/    # generated/shared API client
│   ├── svelte/ui/            # shared Svelte UI helpers
│   └── bazel/                # repo-owned Bazel policy and tooling packages
├── apps/clients/
│   └── aidoku/               # Aidoku client package
├── docs/
│   └── backend-plugins.md    # source plugin authoring guide
├── plugins/
│   ├── comix/                # bundled source plugin
│   └── crates/manga-plugin-kit/
├── third_party/
│   └── rust/                 # single Cargo manifest/lock consumed by rules_rs
├── data/
├── flake.nix
└── start.nu
```

## Requirements

- Nix
- BuildBuddy access for Bazel remote cache and RBE
- Browser Use API key or another remote CDP browser endpoint for challenged sources
- Docker Compose only for the optional observability stack

## Quick Start

Enter the Nix shell, log in to BuildBuddy once, then start the backend and
frontend through Bazel:

```bash
nix develop
bazel login
aspect repo start
```

`aspect repo start` runs the Bazel-built Rust dev launcher, which supervises
`bazel run //apps/rust/server:server -- serve` for the backend and
`pnpm --filter @manga-server/web exec vp dev` for the frontend. The normal
Bazel remote cache, RBE, source-built LLVM, and repo execution-platform defaults
apply to Bazel commands. Build and test commands target Linux x86_64 by default;
`bazel run` targets the host platform so the dev launcher and backend binary run
locally on macOS.

Open:

- Web UI: `http://localhost:5173`
- Backend health: `http://localhost:4000/v1/health`
- Backend build info: `http://localhost:4000/v1/info`
- Aidoku package: `http://localhost:4000/v1/clients/aidoku/package`

## Manual Development

Build and test the migrated Bazel graph:

```bash
nix develop -c aspect repo build
nix develop -c aspect repo test
nix develop -c aspect repo frontend
nix develop -c aspect repo backend
```

Useful backend commands:

```bash
nix develop -c bazel run //apps/rust/server:server -- config print
nix develop -c bazel run //apps/rust/server:server -- plugins list
nix develop -c bazel run //apps/rust/server:server -- db migrate
nix develop -c bazel run //apps/rust/server:server -- openapi export
```

Run quality checks through the Aspect task surface:

```bash
nix develop -c aspect repo quality
nix develop -c aspect repo coverage
```

Legacy Cargo, PNPM, and Nushell entry points still exist while the migration is
in progress, but the Bazel path is the primary development path for this branch.

Clean local outputs from the repo root:

```bash
nu scripts/clean.nu --build --dry-run
nu scripts/clean.nu --build
```

Use `--frontend`, `--runtime`, `--ccc`, or `--all` for broader cleanup. `--runtime` removes local databases and downloaded/cache data, so inspect it with `--dry-run` first.

## Source Plugins

Source plugins are WebAssembly components loaded by the backend from `PLUGINS_PATH`.

The bundled reference plugin is `plugins/comix`. The source contract is `libs/rust/plugin-host/wit/manga-source.wit`; the Rust helper crate is `plugins/crates/manga-plugin-kit`.

Build the bundled plugin through Bazel:

```bash
aspect build //plugins/comix:comix_wasm
```

Cargo and cargo-component builds are intentionally unsupported in this repository. The single Cargo manifest under `third_party/rust` exists only so Bazel/rules_rs can resolve third-party Rust crates. For authoring details, see [Writing Backend Source Plugins](docs/backend-plugins.md).

## Configuration

Manga Server has two configuration layers:

1. Process environment variables for bootstrap and deployment concerns
2. Persisted app settings stored in the backend database and editable from the Settings page

On startup, supported non-empty environment variables are applied to persisted settings, so deployment values can override database values.

### Backend Bootstrap

| Variable | Default | Purpose |
| --- | --- | --- |
| `DB_PATH` | `./data/manga.db` | SQLite database path |
| `SERVER_ADDR` | `0.0.0.0:4000` | Backend bind address |
| `PLUGINS_PATH` | auto-detected | Directory containing `.wasm` plugins |
| `SOURCE_PLUGIN_REGISTRY_URL` | empty | Remote source-plugin registry manifest URL; when set, startup installs the Comix plugin from the latest compatible registry artifact |
| `AIDOKU_PACKAGE_PATH` | built on demand | Prebuilt Aidoku `.aix` package served by `/v1/clients/aidoku/package` |
| `TACHIYOMI_PACKAGE_PATH` | built on demand | Prebuilt Tachiyomi/Mihon `.apk` package served by `/v1/clients/tachiyomi/package` |
| `BACKEND_API_KEY` | empty | Requires `Authorization: Bearer <key>` when set |
| `DISCORD_BOT_TOKEN` | empty | Overrides the persisted Discord bot token setting on startup |
| `DISCORD_CHANNEL_ID` | empty | Overrides the persisted Discord notification channel setting on startup |
| `BROWSER_USE_API_KEY` | empty | Browser Use API key used to run the clearance/capture browser remotely |
| `BROWSER_USE_CONNECT_URL` | `wss://connect.browser-use.com` | Override Browser Use's direct CDP WebSocket endpoint |
| `BROWSER_USE_PROFILE_ID` | empty | Optional Browser Use profile loaded into remote browser sessions |
| `BROWSER_USE_TIMEOUT_MINUTES` | `5` | Browser Use session timeout used as a billing safety limit |
| `MANGA_SERVER_BROWSER_CDP_URL` | empty | Generic remote browser CDP endpoint; takes precedence over Browser Use when set |

### Discord Bot

Configure the Discord card on the Settings page, or set `DISCORD_BOT_TOKEN`, to run the gateway bot and globally register slash commands. Command registration bulk-overwrites the global command list on startup, so renamed or removed manga-server commands are removed from Discord. Set the notification channel in the Settings page, or with `DISCORD_CHANNEL_ID`, to post a message when the scheduler finds new post-baseline library chapters. Discord setting changes take effect after restarting the backend.

| Command | Purpose |
| --- | --- |
| `/manga` | Open manga-server controls with buttons for status, library, downloads, sources, updates, refresh, and help |
| `/manga query:<text>` | Search the saved library by title, source, or category |
| `/settings` | Show persisted manga-server settings with quick action buttons |
| `/settings setting:<key> value:<value>` | Update supported persisted settings from Discord |

### Persisted Settings With Env Overrides

| Setting key | Env var | Default |
| --- | --- | --- |
| `auth_enabled` | `AUTH_ENABLED` | `false` |
| `auth_oidc_issuer_url` | `OIDC_ISSUER_URL` | empty |
| `auth_oidc_client_id` | `OIDC_CLIENT_ID` | empty |
| `auth_oidc_client_secret` | `OIDC_CLIENT_SECRET` | empty |
| `auth_oidc_scopes` | `OIDC_SCOPES` | `openid profile email` |
| `auth_oidc_provider_id` | `OIDC_PROVIDER_ID` | `oidc` |
| `discord_bot_token` | `DISCORD_BOT_TOKEN` | empty |
| `discord_channel_id` | `DISCORD_CHANNEL_ID` | empty |
| `update_interval_hours` | `UPDATE_INTERVAL_HOURS` | `1` |
| `auto_download_new_chapters` | `AUTO_DOWNLOAD_NEW_CHAPTERS` | `false` |
| `auto_download_category` | `AUTO_DOWNLOAD_CATEGORY` | empty |
| `download_path` | `DOWNLOAD_PATH` | `./data/downloads` |
| `download_concurrent_chapters` | `DOWNLOAD_CONCURRENT_CHAPTERS` | `2` |
| `download_page_fetch_concurrency` | `DOWNLOAD_PAGE_FETCH_CONCURRENCY` | `2` |
| `max_download_storage_bytes` | `MAX_DOWNLOAD_STORAGE_BYTES` | empty |
| `library_categories` | `LIBRARY_CATEGORIES` | `default,downloaded` |
| `cache_disk_path` | `CACHE_DISK_PATH` | `./data/cache` |
| `cache_max_memory_bytes` | `CACHE_MAX_MEMORY_BYTES` | `268435456` |
| `avif_conversion_workers` | `AVIF_CONVERSION_WORKERS` | `5` |

### Frontend Server

| Variable | Default | Purpose |
| --- | --- | --- |
| `BACKEND_URL` | `http://localhost:4000` | Backend URL used by the frontend server/proxy. Set this to change the backend host or IP for local runs. |
| `BACKEND_INTERNAL_URL` | `BACKEND_URL` | Server-side backend URL override for deployments with a private backend origin |
| `PUBLIC_API_BASE` | `/api/app` in the browser, `BACKEND_URL` on the server | Browser-visible backend URL for deployments that bypass the frontend proxy |
| `BACKEND_API_KEY` | empty | Bearer token forwarded to protected backend requests |
| `BETTER_AUTH_SECRET` | empty | Better Auth secret |
| `BETTER_AUTH_URL` | request origin | Explicit Better Auth base URL |
| `AUTH_DB_PATH` | `data/auth.db` | Local auth database path, relative to `apps/svelte/web/` |
| `PUBLIC_REPOSITORY_URL` | repo fallback | Repository link shown on the About page |
| `REPOSITORY_URL` | repo fallback | Alternate repository URL source |
| `GIT_BRANCH` | empty | Displayed branch metadata |
| `GIT_COMMIT_SHA` | empty | Full commit SHA |
| `GIT_COMMIT_SHORT_SHA` | derived when possible | Short commit SHA |
| `GIT_COMMIT_AT` | build timestamp fallback | Commit timestamp |
| `GIT_COMMIT_MESSAGE` | empty | Commit subject |
| `GIT_REMOTE_BRANCH` | empty | Remote branch metadata |
| `GIT_REMOTE_COMMIT_SHA` | empty | Remote commit SHA |
| `GIT_REMOTE_COMMIT_SHORT_SHA` | derived when possible | Remote short SHA |

### Bazel Dev Launcher

| Variable or flag | Purpose |
| --- | --- |
| `.env` | Optional ignored workspace file loaded by the dev launcher before spawning the backend/frontend |
| `BROWSER_USE_API_KEY` | Browser Use API key forwarded to the backend |
| `MANGA_SERVER_BROWSER_CDP_URL` | Generic remote browser CDP endpoint forwarded to the backend |
| `DB_PATH` | Backend database path, defaulting to `./data/manga.db` |
| `DOWNLOAD_PATH` | Download directory, defaulting to `./data/downloads` |
| `PLUGINS_PATH` | Plugin directory, defaulting to `./plugins` |
| `--cache-trace` | Enables extra cache and Foyer tracing in `RUST_LOG` |

## Backend API

The backend exposes:

- `/v1/health` for readiness checks
- `/v1/info` for build metadata
- `/v1/sources` for plugin and source management
- `/v1/library` for saved series and chapter metadata
- `/v1/library/updates` for newly discovered chapters
- `/v1/downloads` for the queue, exports, retries, archives, and re-encoding
- `/v1/clients/{client}/package` for generated client packages, currently `aidoku`
- `/v1/settings` for persisted app settings and cache/image maintenance
- `/v1/media/image` for proxied media fetches

The SvelteKit app mirrors the app API through server routes under `apps/svelte/web/src/routes/api/app`.

## Observability

The backend exports OpenTelemetry traces, logs, and metrics over OTLP. Telemetry covers HTTP request spans and metrics, tracing events/logs, API error responses, source/plugin operations, source cache hit/miss behavior, plugin lifecycle changes, download enqueue/completion/page-fetch behavior, archive sizes, Foyer cache behavior, media proxy traffic, search cache warming, Discord notifications, Tokio runtime health, and library update runs.

Start the preconfigured local OTLP/Grafana stack:

```bash
bazel build //observability:generated_config
cd bazel-bin/observability/generated
docker compose up
```

The local stack uses VictoriaMetrics for metrics, VictoriaLogs for logs, Grafana for dashboards, an OpenTelemetry Collector as the OTLP receiver, and Tempo for traces. It provisions the repository dashboards automatically:

- `Manga Server Overview` for HTTP, download, source/plugin, cache, storage, archive, scheduler, runtime, and API error metrics
- `Manga Server Signals` for VictoriaLogs logs and Tempo trace search
- `Manga Server Tokio Runtime` for scheduler and runtime pressure
- Autometrics overview and function explorer dashboards

Run the backend with OTLP enabled:

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 \
OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf \
OTEL_RESOURCE_ATTRIBUTES=service.namespace=manga,deployment.environment=local,service.instance.id=local-dev \
aspect run //apps/rust/server:server
```

Set `OTEL_SDK_DISABLED=true` when you need to run the backend without exporting telemetry.

Open:

- Grafana: `http://localhost:3000` with `admin` / `admin`
- OTLP HTTP receiver: `http://localhost:4318`
- OTLP gRPC receiver: `http://localhost:4317`

## Frontend Deployment

The frontend uses `@sveltejs/adapter-cloudflare` and can be deployed to Cloudflare Workers independently from the Rust backend.

Build and deploy from the repo root:

```bash
pnpm install
pnpm build
pnpm --filter @manga-server/web exec wrangler deploy
```

Run a local Workers preview:

```bash
pnpm run preview:cloudflare
```

Common Worker values:

- `BACKEND_URL`
- `PUBLIC_API_BASE`
- `BACKEND_INTERNAL_URL`
- `BACKEND_API_KEY`
- `BETTER_AUTH_SECRET`
- `PUBLIC_REPOSITORY_URL`
- `GIT_BRANCH`
- `GIT_COMMIT_SHA`
- `GIT_COMMIT_SHORT_SHA`
- `GIT_COMMIT_AT`
- `GIT_COMMIT_MESSAGE`
- `GIT_REMOTE_BRANCH`
- `GIT_REMOTE_COMMIT_SHA`
- `GIT_REMOTE_COMMIT_SHORT_SHA`

When auth is enabled on Workers, add a D1 binding named `AUTH_DB` in `apps/svelte/web/wrangler.toml`.

## NixOS Service

The flake exports a NixOS module that runs the backend as `manga-server.service`.

```nix
{
  inputs.manga-server.url = "github:TrashCan69420/manga-downloader";

  outputs =
    inputs@{ nixpkgs, ... }:
    {
      nixosConfigurations.example = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          inputs.manga-server.nixosModules.manga-server
          {
            services.manga-server = {
              enable = true;
              openFirewall = true;
            };
          }
        ];
      };
    };
}
```

By default the service listens on `0.0.0.0:4000`, stores state under `/var/lib/manga-server`, and copies bundled plugins into `/var/lib/manga-server/plugins`. Browser automation requires either `BROWSER_USE_API_KEY` or `MANGA_SERVER_BROWSER_CDP_URL`.

Common options:

```nix
services.manga-server = {
  enable = true;
  host = "127.0.0.1";
  port = 4000;
  stateDir = "/var/lib/manga-server";
  environmentFiles = [ "/run/secrets/manga-server.env" ];
};
```

An environment file can contain:

```bash
BACKEND_API_KEY=change-me
```

## Nix Launcher

Nix provides the Aspect/Bazel launcher environment only. It no longer exposes
Cargo-built packages for the backend, plugins, or clients.

```bash
nix develop --accept-flake-config -c aspect repo doctor
nix develop --accept-flake-config -c aspect repo quality
nix build .#cargo-manifest-policy
```

## Development Notes

- Database migrations run automatically on backend startup.
- `aspect repo start -- --cache-trace` enables extra cache tracing in the Bazel dev launcher.
- `aspect repo frontend` builds and tests the migrated frontend Bazel targets.
- Shared frontend API calls and response schemas live in `libs/svelte/api-client`.
- Auth is optional. When enabled, login uses Better Auth with a generic OIDC provider.
