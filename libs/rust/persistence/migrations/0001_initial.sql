CREATE TABLE "app_settings" (
    "key" TEXT NOT NULL,
    "value" TEXT NOT NULL,
    PRIMARY KEY ("key")
);
-- #[toasty::breakpoint]
CREATE TABLE "sources" (
    "key" TEXT NOT NULL,
    "display_name" TEXT NOT NULL,
    "base_url" TEXT NOT NULL,
    "version" TEXT NOT NULL,
    "enabled" BOOLEAN NOT NULL,
    "status" TEXT NOT NULL,
    "status_reason" TEXT,
    "capabilities" TEXT NOT NULL,
    "plugin_api_version" BIGINT NOT NULL,
    "plugin_api_min_supported" BIGINT NOT NULL,
    "plugin_api_max_supported" BIGINT NOT NULL,
    "hide_nsfw" BOOLEAN NOT NULL,
    "installed_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    PRIMARY KEY ("key")
);
-- #[toasty::breakpoint]
CREATE TABLE "library_series" (
    "id" TEXT NOT NULL,
    "source_key" TEXT NOT NULL,
    "remote_series_id" TEXT NOT NULL,
    "title" TEXT NOT NULL,
    "cover_url" TEXT NOT NULL,
    "cover_fetch_spec" TEXT,
    "description" TEXT NOT NULL,
    "author" TEXT NOT NULL,
    "genres" TEXT NOT NULL,
    "status" TEXT NOT NULL,
    "category" TEXT NOT NULL,
    "is_nsfw" BOOLEAN NOT NULL,
    "auto_download_new" BOOLEAN,
    "language" TEXT,
    "chapters_initialized" BOOLEAN NOT NULL,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
CREATE INDEX "index_library_series_by_source_key" ON "library_series" ("source_key");
-- #[toasty::breakpoint]
CREATE INDEX "index_library_series_by_remote_series_id" ON "library_series" ("remote_series_id");
-- #[toasty::breakpoint]
CREATE TABLE "chapters" (
    "id" TEXT NOT NULL,
    "series_id" TEXT NOT NULL,
    "remote_chapter_id" TEXT NOT NULL,
    "title" TEXT NOT NULL,
    "number" BIGINT NOT NULL,
    "published_at" TEXT NOT NULL,
    "is_new" BOOLEAN NOT NULL,
    "fetched_at" TEXT NOT NULL,
    PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
CREATE INDEX "index_chapters_by_series_id" ON "chapters" ("series_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_chapters_by_remote_chapter_id" ON "chapters" ("remote_chapter_id");
-- #[toasty::breakpoint]
CREATE TABLE "downloads" (
    "id" TEXT NOT NULL,
    "series_id" TEXT NOT NULL,
    "chapter_id" TEXT NOT NULL,
    "status" TEXT NOT NULL,
    "progress_percent" BIGINT NOT NULL,
    "stage" TEXT NOT NULL,
    "error_code" TEXT,
    "error_message" TEXT,
    "file_path" TEXT,
    "file_size_bytes" BIGINT,
    "attempt_count" BIGINT NOT NULL,
    "queued_at" TEXT NOT NULL,
    "started_at" TEXT,
    "finished_at" TEXT,
    PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
CREATE INDEX "index_downloads_by_series_id" ON "downloads" ("series_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_downloads_by_chapter_id" ON "downloads" ("chapter_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_downloads_by_status" ON "downloads" ("status");
-- #[toasty::breakpoint]
CREATE INDEX "index_downloads_by_queued_at" ON "downloads" ("queued_at");
-- #[toasty::breakpoint]
CREATE TABLE "download_events" (
    "id" TEXT NOT NULL,
    "download_id" TEXT NOT NULL,
    "event_type" TEXT NOT NULL,
    "message" TEXT NOT NULL,
    "payload_json" TEXT,
    "created_at" TEXT NOT NULL,
    PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
CREATE INDEX "index_download_events_by_download_id" ON "download_events" ("download_id");
-- #[toasty::breakpoint]
CREATE TABLE "plugin_artifacts" (
    "id" TEXT NOT NULL,
    "plugin_key" TEXT NOT NULL,
    "source_key" TEXT,
    "plugin_version" TEXT NOT NULL,
    "artifact_path" TEXT NOT NULL,
    "sha256" TEXT NOT NULL,
    "wasm_component_valid" BOOLEAN NOT NULL,
    "wit_world_valid" BOOLEAN NOT NULL,
    "descriptor_valid" BOOLEAN NOT NULL,
    "compatible" BOOLEAN NOT NULL,
    "compatibility_reason" TEXT,
    "plugin_api_version" BIGINT NOT NULL,
    "min_host_api_version" BIGINT NOT NULL,
    "max_host_api_version" BIGINT NOT NULL,
    "is_active" BOOLEAN NOT NULL,
    "installed_at" TEXT NOT NULL,
    "replaced_at" TEXT,
    PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
CREATE INDEX "index_plugin_artifacts_by_plugin_key" ON "plugin_artifacts" ("plugin_key");
-- #[toasty::breakpoint]
CREATE INDEX "index_plugin_artifacts_by_is_active" ON "plugin_artifacts" ("is_active");
