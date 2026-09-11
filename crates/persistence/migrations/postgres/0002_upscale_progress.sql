CREATE TABLE "upscale_progress" (
    "download_id" TEXT NOT NULL,
    "status" TEXT NOT NULL,
    "completed_pages" BIGINT NOT NULL,
    "total_pages" BIGINT NOT NULL,
    "message" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    PRIMARY KEY ("download_id")
);
