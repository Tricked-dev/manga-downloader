CREATE TABLE "api_tokens" (
    "id" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "token_hash" TEXT NOT NULL,
    "created_at" TEXT NOT NULL,
    "last_used_at" TEXT,
    PRIMARY KEY ("id")
);

CREATE UNIQUE INDEX "index_api_tokens_by_token_hash" ON "api_tokens" ("token_hash");
