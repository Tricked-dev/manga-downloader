# Browser integration test

This test drives Chromium through a real Dex authorization-code login, navigates the built
application, saves and restores an upscale setting, creates and revokes an isolated public
share, and signs out through the mobile menu. It uses the running Rust API, with no API mocks.
Use a separate database: the test creates and deletes a manga entry and changes a setting.

Build the web application and server first. Then start these in separate terminals from the
repository root, with `dex` on PATH:

```sh
mkdir -p /tmp/manga-ui-e2e/models
dex serve web/e2e/dex.yaml
```

```sh
AUTH_ENABLED=true \
OIDC_ISSUER_URL=http://127.0.0.1:5556/dex \
OIDC_CLIENT_ID=manga-ui OIDC_CLIENT_SECRET=browser-fixture-client \
PUBLIC_URL=http://localhost:4000 \
DOWNLOAD_PATH=/tmp/manga-ui-e2e/downloads \
CACHE_DISK_PATH=/tmp/manga-ui-e2e/cache \
cargo run -p manga-server -- serve \
  --database-url /tmp/manga-ui-e2e/manga.db \
  --models-dir /tmp/manga-ui-e2e/models --addr 127.0.0.1:4000
```

```sh
CHROMIUM_EXECUTABLE_PATH="$(command -v chromium)" bun run --cwd web test:e2e
```

The fixture account is `reader@example.test` / `browser-fixture-password`. These are disposable
test credentials. Dex and the server bind only to loopback, and the empty model directory
prevents inference during this UI test. The test also accepts `E2E_BASE_URL`, `E2E_EMAIL` and
`E2E_PASSWORD` for a separate Dex fixture. Register its callback as `/auth/callback` on the
selected external origin. Omit `CHROMIUM_EXECUTABLE_PATH` to use Playwright's installed Chromium.

To test a build without embedded assets, add `--web-root "$PWD/web/build"` to the server
command and use `cargo run --no-default-features`.
