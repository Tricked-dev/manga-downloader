# Manga Downloader Aidoku Source

This is an Aidoku source that browses Manga Downloader library categories and searches both downloaded manga and enabled remote sources.

## Settings

- `Server Base URL`: backend URL, defaulting to `http://localhost:4000`.
- `Backend API Key`: optional `BACKEND_API_KEY` value, if your backend requires one.

On a physical iOS device, set `Server Base URL` to an address reachable from that device, such as a LAN IP or tunnel URL.

Browse listings mirror the web UI's `library_categories` setting and include an `All Categories` entry. Remote sources are not shown as browse listings; they are only queried from Aidoku search. Search results exclude already-downloaded manga from the same source. Aidoku does not report or display Manga Downloader read progress; reading state is tracked by the web UI.

Search filters are source-aware. The header shows sort, type, demographic, and downloaded-library chips. Sort filters are mapped to each source only when that source advertises compatible search categories or popular-search support.

## Endpoints Used

- `GET /v1/library`
- `GET /v1/library/{id}`
- `GET /v1/library/{id}/chapters`
- `GET /v1/library/chapters/{id}/pages`
- `GET /v1/library/chapters/{id}/pages/{page}`
- `GET /v1/sources`
- `GET /v1/sources/{source}/search?q={query}&page={page}`
- `GET /v1/sources/{source}/manga/{id}`
- `GET /v1/sources/{source}/manga/{id}/chapters`
- `GET /v1/sources/{source}/chapters/{id}/pages`

## Build

The source is an isolated Cargo package pinned to the official Aidoku Rust SDK. In the development environment:

```sh
cargo build --manifest-path clients/aidoku/Cargo.toml --locked --target wasm32-unknown-unknown --release
cargo run -p manga-server -- clients package aidoku --output /tmp/manga-downloader.aix
```

The server packages the WASM, icon and JSON descriptors into an `.aix`. `GET /v1/clients/aidoku/package` serves the same package. Set `AIDOKU_PACKAGE_PATH` to override it with a prebuilt file; release binaries can embed it with `MANGA_EMBED_AIDOKU_PACKAGE` at build time. Otherwise, the endpoint builds from this checkout and requires Cargo with the WASM target.

Run `aidoku verify /tmp/manga-downloader.aix` using the official Aidoku CLI to validate the package. Install it in Aidoku, set the reachable server URL and bearer API key, then check browse, search, and original/upscaled downloaded pages. Package verification does not replace that device check.
