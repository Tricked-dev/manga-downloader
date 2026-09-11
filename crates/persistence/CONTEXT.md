# Persistence

`Database::open` selects SQLite for file paths and SQLite URLs, or PostgreSQL for `postgres://`
and `postgresql://` URLs. The public domain operations use the same Toasty models. SQLite remains
the default and serializes writes with a process-local lock; PostgreSQL permits concurrent writes.

Each backend has a generated initial schema under `migrations/`. This is a fresh database boundary:
there is no migration from legacy plugin/archive-index tables. Chapter payloads and their indexes
live in BBF; database rows track library identity, download state, read progress and successful
upscale metadata. Timestamps retain their text representation.

The upscale queue lives in the selected application database. The server uses its SQLite queue
backend for a file database and Apalis PostgreSQL with `LISTEN/NOTIFY` for PostgreSQL. Browser
sessions store hashed opaque tokens, identity, expiry and an authentication-configuration hash;
public-share records identify a specific library series.

Database tests create isolated SQLite files by default. `TEST_POSTGRES_URL` selects an
isolated PostgreSQL database for each server fixture. Both schema generation and PostgreSQL
connections initialize the application's Rustls provider before connecting.

Schema changes use numbered migrations without modifying applied migrations. The schema generator writes schema.sql snapshots for comparison. Upscale page progress is stored separately from download work state.
