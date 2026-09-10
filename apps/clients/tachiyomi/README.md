# Manga Downloader Tachiyomi Source

This is a standalone Tachiyomi/Mihon source extension for Manga Downloader. It builds directly from this directory and does not require cloning Keiyoushi or another extension-source repository.

## Settings

- `Server Base URL`: backend URL, defaulting to `http://localhost:4000`.
- `Source Names`: optional comma-separated Manga Downloader source plugin names. Leave blank to auto-discover enabled searchable sources; if discovery is unavailable, it falls back to `comix`.
- `Backend API Key`: optional `BACKEND_API_KEY` value, if your backend requires one.

On Android Emulator, use `http://10.0.2.2:4000` for a server running on the host machine. On a physical Android device, use a LAN IP or tunnel URL reachable from that device.

## Endpoints Used

- `GET /v1/library`
- `GET /v1/library/{id}`
- `GET /v1/library/{id}/chapters`
- `GET /v1/library/chapters/{id}/pages`
- `GET /v1/sources`
- `GET /v1/sources/{source}/search?q={query}&page={page}`
- `GET /v1/sources/{source}/manga/{id}`
- `GET /v1/sources/{source}/manga/{id}/chapters`
- `GET /v1/sources/{source}/chapters/{id}/pages`

## Build

```bash
bazel build //apps/clients/tachiyomi:source
```

The Bazel target uses `rules_kotlin` and `rules_java` to compile the extension sources against compile-only stubs. APK packaging still needs a hermetic Android SDK/package target before it should be added to the build graph.
