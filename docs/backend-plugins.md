# Writing Backend Source Plugins

Backend source plugins are WebAssembly components loaded by Manga Server at startup or through the source upload endpoint. A plugin is responsible for one manga source: search, manga details, chapters, and page images. The backend handles storage, downloads, proxying, caching, auth, and UI integration.

The reference implementation is `plugins/comix`. Copy its shape unless you have a reason not to.

## Runtime Contract

The contract lives in `libs/rust/plugin-host/wit/manga-source.wit`. Plugins implement the `manga:source/manga-source` world and export:

- `metadata()`
- `search(query)`
- `get-manga(manga-id)`
- `get-chapters(manga-id)`
- `get-pages(chapter-id)`

The host imports available to plugins are:

- `fetch(request)`, for text HTTP requests through the backend's shared client
- `capture-browser-json(request)`, for browser-backed JSON capture on Cloudflare-protected pages

The current backend plugin API version is `6`. In Rust plugins, use `manga_plugin_kit::prelude::CURRENT_PLUGIN_API_VERSION` through `SourceManifestBuilder` instead of hard-coding it.

## Recommended Rust Layout

Create a Rust `cdylib` crate under `plugins/`:

```toml
[package]
name = "my-source"
version = "0.1.0"
edition = "2024"

[dependencies]
wit-bindgen = "0.57.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tracing = "0.1"
manga-plugin-kit = { path = "../crates/manga-plugin-kit" }

[lib]
crate-type = ["cdylib"]
```

Generate bindings from the shared WIT file and export one guest type:

```rust
wit_bindgen::generate!({
    path: "../../libs/rust/plugin-host/wit/manga-source.wit",
    world: "manga-source",
});

pub struct MySource;

export!(MySource);
```

If your crate is not exactly two directories below the repo root, adjust the WIT path. Keep a `build.rs` that reruns when the WIT file changes:

```rust
fn main() {
    println!("cargo::rerun-if-changed=src");
    println!("cargo::rerun-if-changed=../../libs/rust/plugin-host/wit/manga-source.wit");
}
```

## Metadata

The backend inspects every `.wasm` component by calling `metadata()` before the plugin is registered. Bad metadata means the plugin will not load.

Metadata should be stable:

- `id` is the backend source key. Changing it creates a new source from the backend's point of view.
- `name` is the display name shown to users.
- `base-url` is the source root used for source settings and logging.
- `version` is the plugin version, not the backend version.
- `api-version` must match the backend plugin API.
- `capabilities` should list only the operations the plugin actually supports.
- `search-options` controls categories, the default category, and popular-sort support in clients.
- `homepage`, `repository`, and `build-metadata` are optional but useful for artifact inspection.

With `manga-plugin-kit`, build the manifest first and map it into the generated WIT type:

```rust
use manga_plugin_kit::prelude::{SourceCapability as KitCapability, SourceManifestBuilder};

use crate::manga::source::types::{SearchOptions, SourceCapability, SourceMetadata};

fn metadata() -> SourceMetadata {
    let manifest = SourceManifestBuilder::new("my-source", "My Source", BASE_URL, PLUGIN_VERSION)
        .search()
        .manga_details()
        .chapter_list()
        .page_list()
        .search_categories(["best_match", "updated_date"])
        .default_search_category("best_match")
        .supports_popular_sort()
        .homepage(BASE_URL)
        .repository("https://github.com/you/my-source")
        .build();

    SourceMetadata {
        id: manifest.id,
        name: manifest.name,
        base_url: manifest.base_url,
        version: manifest.version,
        api_version: manifest.api_version,
        build_metadata: manifest.build_metadata,
        homepage: manifest.homepage,
        repository: manifest.repository,
        capabilities: manifest
            .capabilities
            .into_iter()
            .map(to_source_capability)
            .collect(),
        search_options: SearchOptions {
            categories: manifest.search_options.categories,
            default_category: manifest.search_options.default_category,
            supports_popular_sort: manifest.search_options.supports_popular_sort,
        },
    }
}

fn to_source_capability(capability: KitCapability) -> SourceCapability {
    match capability {
        KitCapability::Search => SourceCapability::Search,
        KitCapability::MangaDetails => SourceCapability::MangaDetails,
        KitCapability::ChapterList => SourceCapability::ChapterList,
        KitCapability::PageList => SourceCapability::PageList,
    }
}
```

The `comix` plugin has the complete conversion code in `plugins/comix/src/source.rs`.

## Implementing Operations

Use these return types from the generated bindings:

- `search` returns `SearchResults { items, has_next_page }`
- `get-manga` returns one `Manga`
- `get-chapters` returns a `Vec<Chapter>`
- `get-pages` returns a `Vec<Page>`

Keep IDs opaque but stable. The backend stores `manga.id` and `chapter.id`, then passes them back to later plugin calls. If a source uses URL slugs for manga and numeric IDs for chapters, return those exact values and parse them inside the plugin.

Errors should use `PluginError`:

- `code`: stable machine-readable category, for example `search_failed` or `manga_not_found`
- `message`: useful operator-facing detail
- `retryable`: `true` for transient HTTP, rate limit, and clearance failures; `false` for missing manga, parse assumptions that need a code change, or unsupported inputs

Do not panic for source failures. Return a `PluginError` so the backend can log and surface the problem.

## Host HTTP

Plugins should use the imported `fetch()` function instead of creating their own network client. The host client provides:

- A shared browser-like user agent
- Cookies
- Timeouts
- Retry and request coalescing
- Cloudflare/DDOS challenge clearance
- Request-purpose hints for API, document, image, asset, and custom requests

The helper crate gives small constructors:

```rust
use manga_plugin_kit::prelude::{api_json, get_html, media_hotlink};
```

Use `api_json()` for JSON endpoints, `get_html()` for pages, and `media_hotlink()` when an image needs a `Referer`. Return page images as `MediaRef` values. The backend fetches those images later through the media proxy or downloader, so plugin page listing should stay lightweight.

For sources that only reveal chapter/page data in the browser, use the `capture-browser-json` import. Keep scripts narrow and bounded: set a timeout, poll interval, done expression, and payload extraction expression. Browser capture is intentionally more limited than normal HTTP.

## Build

Build plugins through Bazel component targets:

```bash
aspect build //plugins/my-source:my_source_wasm
```

The compiled component should be under:

```text
target/wasm32-wasip1/release/my_source.wasm
```

Copy it into the active plugin directory:

```bash
mkdir -p ../../plugins
cp target/wasm32-wasip1/release/my_source.wasm ../../plugins/my-source.wasm
```

Cargo and cargo-component builds are intentionally unsupported in this repository. The single Cargo manifest under `third_party/rust` is a rules_rs dependency manifest, not a source workspace. New bundled plugins need Bazel package targets before they participate in the build.

## Install And Reload

The backend loads every `.wasm` file under `PLUGINS_PATH`. You can point the backend at a scratch directory:

```bash
PLUGINS_PATH=plugins aspect run //apps/rust/server:server
```

List loaded sources:

```bash
PLUGINS_PATH=plugins aspect run //apps/rust/server:server -- plugins list
```

Reload sources without restarting the server:

```bash
curl -X POST http://localhost:4000/v1/sources/reload
```

Upload a plugin through the API:

```bash
curl -F "file=@plugins/my-source.wasm" http://localhost:4000/v1/sources/upload
```

If `BACKEND_API_KEY` is set, add:

```bash
-H "Authorization: Bearer $BACKEND_API_KEY"
```

The upload path validates the component, calls `metadata()`, writes the artifact to `PLUGINS_PATH`, records artifact history, and replaces an existing source when the metadata `id` matches.

## Test Checklist

Before calling a plugin usable:

- `metadata()` loads and reports API version `6`
- `plugins list` shows the source key, display name, version, and enabled state
- `/v1/sources` includes the expected capabilities and search options
- Search works for page `1`, an empty query if the source supports browse-like search, and a query with no results
- `get-manga` returns a cover `MediaRef` that the backend can proxy
- `get-chapters` returns stable chapter IDs and parseable numbers
- `get-pages` returns page indices in reading order
- Media refs with hotlink requirements include the needed request headers
- Transient upstream failures return `retryable: true`
- Not-found cases return `retryable: false`

Useful manual probes:

```bash
curl http://localhost:4000/v1/sources
curl "http://localhost:4000/v1/sources/my-source/search?q=one&page=1"
curl "http://localhost:4000/v1/sources/my-source/manga/$MANGA_ID"
curl "http://localhost:4000/v1/sources/my-source/manga/$MANGA_ID/chapters"
curl "http://localhost:4000/v1/sources/my-source/chapters/$CHAPTER_ID/pages"
```

## Notes For Bundled Plugins

Registry-installed plugins need more than a Rust crate:

- Publish a registry manifest whose artifact entries are pinned with SHA-256.
- Publish each `.wasm` artifact at the URL recorded in that manifest.
- Set `SOURCE_PLUGIN_REGISTRY_URL` for the server deployment.
- Keep the plugin metadata `id`, `version`, and `api-version` aligned with the registry entry.
- Keep `libs/rust/plugin-host/wit/manga-source.wit` in the plugin source closure if the artifact is built in this repository.

The current registry-installed source is `comix`. On startup, the backend fetches the configured registry manifest, resolves the latest non-deprecated Comix artifact compatible with the host plugin API, verifies the downloaded `.wasm` against its SHA-256 digest, and installs it into `PLUGINS_PATH`.
