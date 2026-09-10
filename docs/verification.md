# Migration verification

This records the checks performed on 2026-09-10. The release was built and run on x86_64 Linux.
The Apple Silicon Flake outputs also evaluated successfully; that is not a macOS runtime test.

## Build and database checks

- `nix build` produced the server with embedded static web assets and reader packages.
- `nix flake check` passed the release build, 126 Rust tests, Clippy with warnings denied,
  and formatting. The service-dependent PostgreSQL contract is excluded from this sandbox pass.
- The `devenv` PostgreSQL profile passed 127 tests, including the explicit database contract.
  The test command checks the server's data directory before creating disposable test databases.
- The release CLI migrated PostgreSQL, reported `database_backend: postgres`, and created the
  application tables. SQLite remains the default and was exercised by the hermetic suite and live server.
- The final binary links ONNX Runtime 1.27.1 and libavif 1.4.2. CPU model inference was confined to Beru (`192.168.50.18`).
- Svelte checking reported zero errors or warnings. The frontend and generated API client passed
  142 tests. The existing two browser component tests and five source-capture browser cases also passed.

## Live sources and native pages

Both sources were searched through the Rust HTTP API, added to the library, refreshed and downloaded
through the real queue with browser capture available. The downloaded chapters contained:

| Source | Pages | Original BBF size | Sample original page |
| --- | ---: | ---: | --- |
| Rawkuma | 14 | 4,036,593 bytes | JPEG, 836 × 1200 |
| Comix | 75 | 3,074,368 bytes | WebP, 700 × 220 |

The vendored `bbfmux` verified the original page hashes in both archives. Sample pages fetched
independently through Chromium matched the saved bytes exactly. The release API returned the same
native bytes and dimensions. Explicit AVIF, WebP and JPEG conversions kept those dimensions and
returned `X-Image-Cache: MISS`, then `HIT` with identical bytes. Explicit width 400 produced the
requested resize; specifying `variant=original` retained the saved original.

Downloading with an empty models directory completed normally and left readable originals.
Missing-model jobs finished without retrying. Separate tests cover a missing matching model band,
the 2000-pixel chapter-quality cutoff, interrupted appends, variant replacement, mapped readers,
and cache invalidation after an append.

## Browser authentication and Aidoku

A real Dex 2.45.1 issuer completed the browser PKCE login against Rust. Browser tests covered the
eight main pages, a cookie-authenticated settings update, public sharing and revocation, mobile
navigation and logout. The same flow passed against the Nix release's embedded assets.
The SQLite session remained valid across a server restart.

The Aidoku source compiled to WASM, passed its seven host tests, and was packaged through both
the on-demand server command and the Nix release. The official Aidoku validator accepted the
package served by the Nix release: minimum API version, required exports, icon and resource schemas.
Aidoku device testing is deferred to the user after deployment on Beru. Further Mihon work and
its device check are out of scope at the user's request.

## Measured CPU upscaling

The same 836 × 1200 Rawkuma page was processed on Beru with explicit CPU inference, two inference
threads, a two-core CPU quota, a 6 GiB memory limit, 256-pixel tiles and 32-pixel overlap. Both runs
used the 1200p MangaJaNai ESRGAN 70k model for their requested scale and the same lossless AVIF encoder.

| Measure | 2× | 4× |
| --- | ---: | ---: |
| Worker elapsed time | 120.380 s | 459.781 s |
| Output dimensions | 1672 × 2400 | 3344 × 4800 |
| Output size | 5,406,648 bytes | 19,249,966 bytes |
| Peak memory | 455 MiB | about 1.1 GiB |

The 4× sample took 3.82 times as long. Automatic jobs remain at 2×, as requested.
These measurements include model loading, inference and AVIF encoding; they are not GPU timings.

The complete 14-page Rawkuma chapter then finished at 2× in **27 minutes 10 seconds**, with a
peak memory footprint of about 500 MiB under the same two-core quota. The finished BBF is
74,792,086 bytes and contains 14 original pages plus 14 upscaled pages in two sections.
The reference BBF tool verified all 28 page hashes. Every original asset retained its hash,
offset and size, including the cover. Live HTTP checks read every original at 836 × 1200
and every upscaled page at 1672 × 2400; the default variant selected the upscaled AVIF.
The database recorded the model, scale and completion time, and an explicit width 1200
still resized on request.

The fixed-output web package was also rebuilt from two different Nix source paths with the
same output hash. Its version and CSS scope identifiers derive from content and relative paths.
