# Web application

The UI is a static SvelteKit application. Bun manages this workspace and the API-client and UI
packages under `packages/`. Rust serves its built assets and owns API authentication, sessions
and public sharing. All browser API calls use the same origin; the Vite development proxy
forwards `/v1` and `/auth` to port 4000.

The root loader reads `/auth/session` before protected child loaders run. A session identity
change clears the browser query cache. Shared library and reader routes use the scoped sharing
cookie; Rust verifies the actual chapter ownership, and the UI hides modification controls and
does not record read progress for a guest.

A completed download always has its original pages. An upscale badge describes a successfully
appended variant; absence of that badge does not make the chapter unreadable. The reader defaults
to the best available stored variant and can request the original explicitly. Neither choice
implies resizing. Automatic upscaling has global and source defaults plus a per-series override. Explicit series on/off takes precedence over both defaults. The series page polls durable upscale job status and page progress independently of download progress.

`bun run generate:api` exports the server's OpenAPI and runs Orval. Generated code is ignored and
must be produced before a clean frontend build. `bun run build:web` writes `web/build`; the
server's default `web` feature embeds it. `--web-root` overrides the embedded files for iteration.
The build honors `SOURCE_DATE_EPOCH` for a reproducible build timestamp.

Validation includes Svelte type checking, API-client tests, UI unit/component tests and the
[Dex browser integration flow](e2e/README.md). Real chapter and extension-device checks remain
separate acceptance gates.
