CREATE INDEX IF NOT EXISTS "index_library_series_by_title"
ON "library_series" ("title");
-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS "index_chapters_by_published_number_fetched"
ON "chapters" ("published_at", "number", "fetched_at");
-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS "index_chapters_by_last_read"
ON "chapters" ("last_read_at");
-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS "index_chapters_by_series_number_published"
ON "chapters" ("series_id", "number", "published_at");
-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS "index_downloads_by_status_queued"
ON "downloads" ("status", "queued_at");
-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS "index_downloads_by_chapter_status_queued"
ON "downloads" ("chapter_id", "status", "queued_at");
-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS "index_downloads_by_series_status"
ON "downloads" ("series_id", "status");
-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS "index_stats_events_by_created_kind"
ON "stats_events" ("created_at", "kind");
