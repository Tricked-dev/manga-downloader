-- Toasty 0.7 does not generate composite uniqueness or partial indexes.
-- Both database engines enforce these domain invariants independently of locks.
CREATE UNIQUE INDEX "library_series_source_identity" ON "library_series" ("source_key", "remote_series_id");
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "chapter_source_identity" ON "chapters" ("series_id", "remote_chapter_id");
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "one_active_download_per_chapter" ON "downloads" ("chapter_id") WHERE "status" IN ('queued', 'fetching_pages', 'downloading_assets', 'transforming_assets', 'archiving', 'cancel_requested');
