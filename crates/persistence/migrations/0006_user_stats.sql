ALTER TABLE "downloads" ADD COLUMN "page_count" BIGINT NOT NULL DEFAULT 0;
-- #[toasty::breakpoint]
CREATE TABLE "stats_events" (
  "id" TEXT NOT NULL,
  "kind" TEXT NOT NULL,
  "source" TEXT,
  "series_id" TEXT,
  "chapter_id" TEXT,
  "amount" BIGINT NOT NULL,
  "created_at" TEXT NOT NULL,
  PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
CREATE INDEX "index_stats_events_by_kind" ON "stats_events" ("kind");
-- #[toasty::breakpoint]
CREATE INDEX "index_stats_events_by_source" ON "stats_events" ("source");
-- #[toasty::breakpoint]
CREATE INDEX "index_stats_events_by_series_id" ON "stats_events" ("series_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_stats_events_by_chapter_id" ON "stats_events" ("chapter_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_stats_events_by_created_at" ON "stats_events" ("created_at");
