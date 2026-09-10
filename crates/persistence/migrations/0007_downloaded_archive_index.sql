CREATE TABLE "downloaded_archive_index" (
    "archive_path" TEXT NOT NULL,
    "archive_size" BIGINT NOT NULL,
    "archive_mtime_ms" BIGINT NOT NULL,
    "schema_version" BIGINT NOT NULL,
    "page_count" BIGINT NOT NULL,
    "index_postcard" BLOB NOT NULL,
    "indexed_at" TEXT NOT NULL,
    "last_used_at" TEXT,
    PRIMARY KEY ("archive_path")
);
-- #[toasty::breakpoint]
CREATE INDEX "index_downloaded_archive_index_by_schema_version" ON "downloaded_archive_index" ("schema_version");
-- #[toasty::breakpoint]
CREATE INDEX "index_downloaded_archive_index_by_last_used_at" ON "downloaded_archive_index" ("last_used_at");
