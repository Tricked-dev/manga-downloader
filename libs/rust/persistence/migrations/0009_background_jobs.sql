CREATE TABLE "background_jobs" (
    "id" TEXT NOT NULL,
    "queue" TEXT NOT NULL,
    "payload_json" TEXT NOT NULL,
    "status" TEXT NOT NULL,
    "attempt_count" BIGINT NOT NULL,
    "max_attempts" BIGINT NOT NULL,
    "run_after" TEXT NOT NULL,
    "locked_by" TEXT,
    "locked_at" TEXT,
    "last_error" TEXT,
    "created_at" TEXT NOT NULL,
    "updated_at" TEXT NOT NULL,
    PRIMARY KEY ("id")
);
-- #[toasty::breakpoint]
CREATE INDEX "index_background_jobs_by_queue_status_run_after"
ON "background_jobs" ("queue", "status", "run_after");
-- #[toasty::breakpoint]
CREATE INDEX "index_background_jobs_by_status_locked_at"
ON "background_jobs" ("status", "locked_at");
