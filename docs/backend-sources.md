# Native sources

Comix and Rawkuma implement `Source` in `crates/sources`. A source provides metadata, search,
manga details, chapters and ordered pages. Register a new adapter in the static source registry
and rebuild the server. Existing catalog enablement and per-source settings remain persisted.

Use the shared `SourceHttpClient` for HTTP and browser capture. It preserves request headers,
cookies, concurrency limits, coalescing, retries and challenge clearance. Return `MediaRef`
values rather than fetching image bytes while listing pages. The media client applies required
transformations such as Comix descrambling; Rawkuma image references include its Referer.

Keep manga and chapter IDs stable and opaque. Return `SourceError { code, message, retryable }`
for source failures. Browser captures need a bounded timeout and a completion condition that
represents complete data, including empty results and pagination. Do not rewrite Comix's signed
request URLs; let its own browser client produce requests through the pagination controls.

The HTTP API retains `plugin_version` and `plugin_api_version` field names for existing clients.
They do not imply an installable plugin runtime. There is no source upload or artifact registry.

Validate parser fixtures with `cargo test -p backend-sources`, and browser capture behavior with
`node web/e2e/source-capture.mjs`. After a source change, check live search, details, complete
chapter and page lists, image fetching, and a real chapter download through `/v1`.
