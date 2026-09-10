ALTER TABLE "chapters" ADD COLUMN "pages_read" BIGINT NOT NULL DEFAULT 0;
-- #[toasty::breakpoint]
ALTER TABLE "chapters" ADD COLUMN "read_completed" BOOLEAN NOT NULL DEFAULT 0;
-- #[toasty::breakpoint]
ALTER TABLE "chapters" ADD COLUMN "last_read_at" TEXT;
-- #[toasty::breakpoint]
CREATE INDEX "index_chapters_by_series_id_and_read_completed"
ON "chapters" ("series_id", "read_completed");
