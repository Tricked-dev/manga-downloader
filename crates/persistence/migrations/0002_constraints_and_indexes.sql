CREATE TEMP TABLE "_series_merge_map" AS
WITH "ranked" AS (
    SELECT
        "id" AS "duplicate_id",
        FIRST_VALUE("id") OVER (
            PARTITION BY "source_key", "remote_series_id"
            ORDER BY "updated_at" DESC, "created_at" DESC, "id" DESC
        ) AS "canonical_id"
    FROM "library_series"
)
SELECT "duplicate_id", "canonical_id"
FROM "ranked"
WHERE "duplicate_id" <> "canonical_id";
-- #[toasty::breakpoint]
UPDATE "chapters"
SET "series_id" = (
    SELECT "canonical_id"
    FROM "_series_merge_map"
    WHERE "duplicate_id" = "chapters"."series_id"
)
WHERE "series_id" IN (SELECT "duplicate_id" FROM "_series_merge_map");
-- #[toasty::breakpoint]
UPDATE "downloads"
SET "series_id" = (
    SELECT "canonical_id"
    FROM "_series_merge_map"
    WHERE "duplicate_id" = "downloads"."series_id"
)
WHERE "series_id" IN (SELECT "duplicate_id" FROM "_series_merge_map");
-- #[toasty::breakpoint]
DELETE FROM "library_series"
WHERE "id" IN (SELECT "duplicate_id" FROM "_series_merge_map");
-- #[toasty::breakpoint]
DROP TABLE "_series_merge_map";
-- #[toasty::breakpoint]
CREATE TEMP TABLE "_chapter_merge_map" AS
WITH "ranked" AS (
    SELECT
        "c"."id" AS "duplicate_id",
        FIRST_VALUE("c"."id") OVER (
            PARTITION BY "c"."series_id", "c"."remote_chapter_id"
            ORDER BY
                CASE
                    WHEN EXISTS (
                        SELECT 1
                        FROM "downloads" AS "d"
                        WHERE "d"."chapter_id" = "c"."id" AND "d"."status" = 'completed'
                    ) THEN 1
                    ELSE 0
                END DESC,
                "c"."fetched_at" DESC,
                "c"."published_at" DESC,
                "c"."id" DESC
        ) AS "canonical_id"
    FROM "chapters" AS "c"
)
SELECT "duplicate_id", "canonical_id"
FROM "ranked"
WHERE "duplicate_id" <> "canonical_id";
-- #[toasty::breakpoint]
UPDATE "downloads"
SET "chapter_id" = (
    SELECT "canonical_id"
    FROM "_chapter_merge_map"
    WHERE "duplicate_id" = "downloads"."chapter_id"
)
WHERE "chapter_id" IN (SELECT "duplicate_id" FROM "_chapter_merge_map");
-- #[toasty::breakpoint]
DELETE FROM "chapters"
WHERE "id" IN (SELECT "duplicate_id" FROM "_chapter_merge_map");
-- #[toasty::breakpoint]
DROP TABLE "_chapter_merge_map";
-- #[toasty::breakpoint]
CREATE TEMP TABLE "_active_download_duplicates" AS
WITH "ranked" AS (
    SELECT
        "id",
        ROW_NUMBER() OVER (
            PARTITION BY "chapter_id"
            ORDER BY "queued_at" DESC, "id" DESC
        ) AS "row_num"
    FROM "downloads"
    WHERE "status" IN (
        'queued',
        'fetching_pages',
        'downloading_assets',
        'transforming_assets',
        'archiving'
    )
)
SELECT "id"
FROM "ranked"
WHERE "row_num" > 1;
-- #[toasty::breakpoint]
UPDATE "downloads"
SET
    "status" = 'cancelled',
    "stage" = 'cancelled',
    "error_code" = COALESCE("error_code", 'superseded_active_download'),
    "error_message" = COALESCE("error_message", 'Superseded by a newer active download'),
    "finished_at" = COALESCE("finished_at", "started_at", "queued_at")
WHERE "id" IN (SELECT "id" FROM "_active_download_duplicates");
-- #[toasty::breakpoint]
DROP TABLE "_active_download_duplicates";
-- #[toasty::breakpoint]
CREATE TEMP TABLE "_active_artifact_duplicates" AS
WITH "ranked" AS (
    SELECT
        "id",
        ROW_NUMBER() OVER (
            PARTITION BY "plugin_key"
            ORDER BY "installed_at" DESC, "id" DESC
        ) AS "row_num"
    FROM "plugin_artifacts"
    WHERE "is_active" = 1
)
SELECT "id"
FROM "ranked"
WHERE "row_num" > 1;
-- #[toasty::breakpoint]
UPDATE "plugin_artifacts"
SET
    "is_active" = 0,
    "replaced_at" = COALESCE("replaced_at", "installed_at")
WHERE "id" IN (SELECT "id" FROM "_active_artifact_duplicates");
-- #[toasty::breakpoint]
DROP TABLE "_active_artifact_duplicates";
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_library_series_by_source_key_and_remote_series_id_unique"
ON "library_series" ("source_key", "remote_series_id");
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_chapters_by_series_id_and_remote_chapter_id_unique"
ON "chapters" ("series_id", "remote_chapter_id");
-- #[toasty::breakpoint]
CREATE INDEX "index_chapters_by_series_id_and_number"
ON "chapters" ("series_id", "number" DESC, "published_at" DESC);
-- #[toasty::breakpoint]
CREATE INDEX "index_downloads_by_status_and_queued_at"
ON "downloads" ("status", "queued_at");
-- #[toasty::breakpoint]
CREATE INDEX "index_downloads_by_chapter_id_and_status"
ON "downloads" ("chapter_id", "status");
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_downloads_active_by_chapter_unique"
ON "downloads" ("chapter_id")
WHERE "status" IN (
    'queued',
    'fetching_pages',
    'downloading_assets',
    'transforming_assets',
    'archiving'
);
-- #[toasty::breakpoint]
CREATE INDEX "index_download_events_by_download_id_and_created_at"
ON "download_events" ("download_id", "created_at");
-- #[toasty::breakpoint]
CREATE INDEX "index_plugin_artifacts_by_plugin_key_and_installed_at"
ON "plugin_artifacts" ("plugin_key", "installed_at" DESC);
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_plugin_artifacts_active_by_plugin_key_unique"
ON "plugin_artifacts" ("plugin_key")
WHERE "is_active" = 1;
