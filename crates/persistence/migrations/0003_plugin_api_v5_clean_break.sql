CREATE TABLE "_sources_new" (
    "key" TEXT NOT NULL,
    "display_name" TEXT NOT NULL,
    "base_url" TEXT NOT NULL,
    "version" TEXT NOT NULL,
    "enabled" BOOLEAN NOT NULL,
    "capabilities" TEXT NOT NULL,
    "plugin_api_version" BIGINT NOT NULL,
    "hide_nsfw" BOOLEAN NOT NULL,
    "installed_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    PRIMARY KEY ("key")
);
-- #[toasty::breakpoint]
INSERT INTO "_sources_new" (
    "key", "display_name", "base_url", "version", "enabled", "capabilities",
    "plugin_api_version", "hide_nsfw", "installed_at", "updated_at"
)
SELECT
    "key", "display_name", "base_url", "version", "enabled", "capabilities",
    "plugin_api_version", "hide_nsfw", "installed_at", "updated_at"
FROM "sources";
-- #[toasty::breakpoint]
DROP TABLE "sources";
-- #[toasty::breakpoint]
ALTER TABLE "_sources_new" RENAME TO "sources";
-- #[toasty::breakpoint]
CREATE TABLE "_plugin_artifacts_new" (
    "id" TEXT NOT NULL,
    "plugin_key" TEXT NOT NULL,
    "plugin_version" TEXT NOT NULL,
    "artifact_path" TEXT NOT NULL,
    "plugin_api_version" BIGINT NOT NULL,
    "is_active" BOOLEAN NOT NULL,
    "installed_at" TEXT NOT NULL,
    "replaced_at" TEXT,
    PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
INSERT INTO "_plugin_artifacts_new" (
    "id", "plugin_key", "plugin_version", "artifact_path",
    "plugin_api_version", "is_active", "installed_at", "replaced_at"
)
SELECT
    "id", "plugin_key", "plugin_version", "artifact_path",
    "plugin_api_version", "is_active", "installed_at", "replaced_at"
FROM "plugin_artifacts"
WHERE "compatible" = 1;
-- #[toasty::breakpoint]
DROP TABLE "plugin_artifacts";
-- #[toasty::breakpoint]
ALTER TABLE "_plugin_artifacts_new" RENAME TO "plugin_artifacts";
-- #[toasty::breakpoint]
CREATE INDEX "index_plugin_artifacts_by_plugin_key" ON "plugin_artifacts" ("plugin_key");
-- #[toasty::breakpoint]
CREATE INDEX "index_plugin_artifacts_by_is_active" ON "plugin_artifacts" ("is_active");
-- #[toasty::breakpoint]
CREATE INDEX "index_plugin_artifacts_by_plugin_key_and_installed_at"
ON "plugin_artifacts" ("plugin_key", "installed_at" DESC);
-- #[toasty::breakpoint]
CREATE UNIQUE INDEX "index_plugin_artifacts_active_by_plugin_key_unique"
ON "plugin_artifacts" ("plugin_key")
WHERE "is_active" = 1;
