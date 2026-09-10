# Sources

A **Source** is a compiled-in adapter implementing metadata, search, manga details, chapter list, and page list. Comix and Rawkuma are separate catalogs; their manga identities are never linked automatically.

The **Source Registry** owns the compiled set and its enablement state. The database persists enablement and source settings. Startup restores those settings before synchronizing the catalog. Disabled sources reject catalog calls; stored library records remain available.

A **Source Error** carries a code, message, and retryable flag. It remains distinguishable through the API's error mapping. Source calls are asynchronous and tracked through cancellation so shutdown can wait for active work.

The **Source HTTP Client** owns concurrency limits, request coalescing, retries, clearance, and browser captures. The **Source Media Client** fetches media and applies required Comix descrambling. Media references preserve request headers and transformation metadata through the existing proxy token contract.

Rawkuma document requests always use browser capture. Its current `/library/` layout uses `search_term`, `the_page`, and `orderby`; details include ComicSeries JSON-LD, chapters use `#chapter-list`, and reader pages use `[data-image-data]`. The browser serializes reader image URLs into the same JSON shape as older `ts_reader.run` payloads. CDN images use HTTP with a Rawkuma Referer. Fixtures document the live layout inspected on 2026-09-10.

The API retains `plugin_version` and `plugin_api_version` field names for client compatibility. There is no plugin installation, artifact history, registry download, or WASM execution runtime.
