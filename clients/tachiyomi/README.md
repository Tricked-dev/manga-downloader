# Manga Downloader Tachiyomi Source

This is a standalone Tachiyomi/Mihon source extension for Manga Downloader. It builds directly from this directory and does not require cloning Keiyoushi or another extension-source repository.

## Settings

- `Server Base URL`: backend URL, defaulting to `http://localhost:4000`.
- `Source Names`: optional comma-separated Manga Downloader source names. Leave blank to auto-discover enabled searchable sources; if discovery is unavailable, it falls back to `comix`.
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

The development environment provides Android SDK platform 35/build tools 35.0.1, JDK 17 and Kotlin 2.0.21. Kotlin is pinned to a version supported by this SDK's D8 compiler.

```sh
clients/tachiyomi/scripts/build-apk.sh
cargo run -p manga-server -- clients package tachiyomi --output /tmp/manga-downloader.apk
```

The script produces `clients/tachiyomi/build/package.apk`, aligns it, signs it with the checked-in development key and verifies its signature. The Java stubs are compile-only; the host application supplies the runtime classes. This key is for this standalone extension, not a secret production identity.

`GET /v1/clients/tachiyomi/package` serves the APK. Set `TACHIYOMI_PACKAGE_PATH` to use a prebuilt file; release binaries can embed it with `MANGA_EMBED_TACHIYOMI_PACKAGE` at build time. Otherwise, the endpoint invokes the build script and needs the SDK, JDK and Kotlin compiler.

Install the APK and trust the extension in Mihon. Configure the reachable server URL and bearer API key, then check browse, search and downloaded original/upscaled pages. AVIF decoding depends on the Android reader's support; callers can request `?format=jpeg` or `?format=webp` from page routes when needed.
