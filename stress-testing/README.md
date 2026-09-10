# Stress Testing

This folder contains k6 tests for a running manga-server backend. The tests do
not start or restart the backend.

Use k6 v0.57.0 or newer so `.ts` scripts run directly:

```bash
k6 version
```

## Suite Shape

The suite is organized around backend behaviors instead of one script per cache
variant:

| Test                    | Script                        | Purpose                                                                 |
| ----------------------- | ----------------------------- | ----------------------------------------------------------------------- |
| `api-smoke`             | `api-smoke.k6.ts`             | Fast health and JSON API correctness checks                             |
| `reader-journey`        | `reader-journey.k6.ts`        | Web-reader-style journey across library APIs and downloaded page images |
| `cached-pages`          | `page-cache.k6.ts`            | Warmed downloaded-page cache hits                                       |
| `uncached-pages`        | `page-cache.k6.ts`            | Archive extraction/page serving with `skip_page_cache=true`             |
| `bursty-uncached-pages` | `bursty-uncached-pages.k6.ts` | Arrival-rate bursts against uncached downloaded pages                   |
| `cached-media-proxy`    | `media-proxy.k6.ts`           | Warmed `/v1/media/image` cover transforms                               |
| `uncached-media-proxy`  | `media-proxy.k6.ts`           | Media proxy transforms with `skip_cache=true`                           |

Aliases kept for convenience: `common` and `api` map to `api-smoke`, `reader`
maps to `reader-journey`, `cached` maps to `cached-pages`, `uncached` maps to
`uncached-pages` except under the `bursty` profile, and `bursty` maps to
`bursty-uncached-pages`.

## Runner

Run the default benchmark suite:

```bash
nu stress-testing/run-k6.nu --profile bench
```

Profiles:

| Profile  | Purpose                                              |
| -------- | ---------------------------------------------------- |
| `smoke`  | Fast correctness check using low VU counts           |
| `bench`  | Fixed-iteration benchmark snapshots                  |
| `stress` | Duration-based sustained load                        |
| `bursty` | Arrival-rate uncached-page benchmark with jagged RPS |

Filter tests with `--tests`:

```bash
nu stress-testing/run-k6.nu --profile stress --tests api-smoke,reader-journey,cached-pages
```

`--tests all` includes default tests for the selected profile. Media-proxy tests
are opt-in because they can fetch from source image hosts:

```bash
nu stress-testing/run-k6.nu --profile bench --tests all,cached-media-proxy,uncached-media-proxy
```

Run the bursty extraction benchmark:

```bash
nu stress-testing/run-k6.nu --profile bursty
```

Useful wrapper flags:

| Flag                     | Default                 | Purpose                                                  |
| ------------------------ | ----------------------- | -------------------------------------------------------- |
| `--server`               | `http://127.0.0.1:4000` | Backend base URL                                         |
| `--api-key`              | `123`                   | Bearer token passed as `API_KEY` and `BACKEND_API_KEY`   |
| `--summary-dir`          | `/tmp/manga-bench`      | Directory for k6 JSON summaries                          |
| `--skip-health-check`    | false                   | Skip the preflight `/v1/health` check                    |
| `--web-dashboard`        | false                   | Enable the k6 web dashboard on the normal runner command |
| `--web-dashboard-open`   | false                   | Ask k6 to open the dashboard in the default browser      |
| `--web-dashboard-port`   | `5665`                  | Dashboard port for the normal runner command             |
| `--web-dashboard-export` | false                   | Export one HTML report per test                          |

The dashboard subcommand is a shorthand for dashboard flags:

```bash
nu stress-testing/run-k6.nu dashboard --profile stress --tests cached,uncached --open --export
```

## Direct k6 Examples

API smoke:

```bash
SERVER=http://127.0.0.1:4000 \
API_KEY=123 \
REQUESTS=60 \
CONCURRENCY=3 \
SUMMARY_PATH=/tmp/manga-bench/k6-api-smoke.json \
k6 run stress-testing/api-smoke.k6.ts
```

Reader journey:

```bash
SERVER=http://127.0.0.1:4000 \
API_KEY=123 \
REQUESTS=3000 \
CONCURRENCY=24 \
URL_POOL=2000 \
PAGES_PER_JOURNEY=4 \
SUMMARY_PATH=/tmp/manga-bench/k6-reader-journey.json \
k6 run stress-testing/reader-journey.k6.ts
```

Cached downloaded pages:

```bash
SERVER=http://127.0.0.1:4000 \
API_KEY=123 \
REQUESTS=12000 \
CONCURRENCY=32 \
URL_POOL=2000 \
PAGE_CACHE_MODE=cached \
SUMMARY_PATH=/tmp/manga-bench/k6-cached-pages.json \
k6 run stress-testing/page-cache.k6.ts
```

Uncached downloaded pages:

```bash
SERVER=http://127.0.0.1:4000 \
API_KEY=123 \
REQUESTS=8000 \
CONCURRENCY=32 \
URL_POOL=8000 \
PAGE_CACHE_MODE=uncached \
SKIP_PAGE_CACHE=true \
SUMMARY_PATH=/tmp/manga-bench/k6-uncached-pages.json \
k6 run stress-testing/page-cache.k6.ts
```

Bursty uncached downloaded pages:

```bash
SERVER=http://127.0.0.1:4000 \
API_KEY=123 \
URL_POOL=12000 \
SKIP_PAGE_CACHE=true \
BASELINE_RPS=25 \
BURST_RPS=150 \
TROUGH_RPS=5 \
BURST_CYCLES=3 \
SUMMARY_PATH=/tmp/manga-bench/k6-bursty-uncached-pages.json \
k6 run stress-testing/bursty-uncached-pages.k6.ts
```

Cached and uncached media proxy:

```bash
SERVER=http://127.0.0.1:4000 \
API_KEY=123 \
REQUESTS=3000 \
CONCURRENCY=16 \
URL_POOL=200 \
MEDIA_PROXY_MODE=cached \
MEDIA_PROXY_WARMUP=true \
SUMMARY_PATH=/tmp/manga-bench/k6-cached-media-proxy.json \
k6 run stress-testing/media-proxy.k6.ts
```

```bash
SERVER=http://127.0.0.1:4000 \
API_KEY=123 \
REQUESTS=1000 \
CONCURRENCY=8 \
URL_POOL=200 \
MEDIA_PROXY_MODE=uncached \
SUMMARY_PATH=/tmp/manga-bench/k6-uncached-media-proxy.json \
k6 run stress-testing/media-proxy.k6.ts
```

## Environment

| Variable                      | Default                 | Purpose                                                  |
| ----------------------------- | ----------------------- | -------------------------------------------------------- |
| `SERVER`                      | `http://127.0.0.1:4000` | Backend base URL                                         |
| `API_KEY` / `BACKEND_API_KEY` | empty                   | Bearer token for protected APIs                          |
| `REQUESTS`                    | `1000`                  | Total shared iterations when `DURATION` is not set       |
| `CONCURRENCY`                 | `20`                    | Virtual users for closed-model tests                     |
| `DURATION`                    | empty                   | Use `constant-vus` instead of fixed shared iterations    |
| `CHAPTERS`                    | `0`                     | Downloaded chapters to sample; `0` means all             |
| `URL_POOL`                    | `0`                     | Number of page/media URLs to prepare                     |
| `PAGES_PER_JOURNEY`           | `3`                     | Images fetched per reader journey iteration              |
| `THINK_MIN_SECONDS`           | `0`                     | Minimum reader think time                                |
| `THINK_MAX_SECONDS`           | `0`                     | Maximum reader think time                                |
| `PAGE_CACHE_MODE`             | `cached`                | `cached`, `uncached`, or `clear-cache`                   |
| `SKIP_PAGE_CACHE`             | `false`                 | Add `skip_page_cache=true` to downloaded page URLs       |
| `CACHE_CLEAR_EVERY_SECONDS`   | `0`                     | Periodically clear cache from VU 1 in `clear-cache` mode |
| `MEDIA_PROXY_MODE`            | `cached`                | `cached` or `uncached`; uncached adds `skip_cache=true`  |
| `MEDIA_PROXY_FORMAT`          | `avif`                  | Optional media proxy `format`; set empty for original    |
| `MEDIA_PROXY_WIDTH`           | `384`                   | Optional media proxy `width`; set `0` to omit            |
| `MEDIA_PROXY_WARMUP`          | mode-dependent          | Warm media proxy targets before measured requests        |
| `BASELINE_RPS`                | `25`                    | Baseline request rate for bursty uncached pages          |
| `BURST_RPS`                   | `150`                   | Peak request rate for bursty uncached pages              |
| `TROUGH_RPS`                  | `5`                     | Low request rate after each burst                        |
| `BURST_CYCLES`                | `3`                     | Number of burst cycles                                   |
| `BURST_RAMP_DURATION`         | `15s`                   | Ramp segment duration for bursty profile                 |
| `BURST_HOLD_DURATION`         | `30s`                   | Hold segment duration for bursty profile                 |
| `BURST_REST_DURATION`         | `20s`                   | Recovery segment duration for bursty profile             |
| `BURST_COOLDOWN_DURATION`     | `10s`                   | Final ramp-down duration                                 |
| `PRE_ALLOCATED_VUS`           | derived                 | Initial VU pool for arrival-rate tests                   |
| `MAX_VUS`                     | derived                 | Maximum VU pool for arrival-rate tests                   |
| `TIMEOUT`                     | `10s`                   | Request timeout                                          |
| `P95_THRESHOLD`               | workload-specific       | k6 p95 threshold                                         |
| `P99_THRESHOLD`               | workload-specific       | k6 p99 threshold                                         |
| `SUMMARY_PATH`                | empty                   | Optional full JSON summary output path                   |
