# Manga Downloader Aidoku Source

This is an Aidoku source that browses Manga Downloader library categories and searches both downloaded manga and configured remote source plugins.

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

Cargo builds are disabled for this repository. Add a hermetic Bazel wasm package target before shipping an Aidoku artifact from this tree.
