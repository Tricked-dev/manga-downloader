# Database backends

`DATABASE_URL` or `--database-url` selects the application database and durable upscale queue. A bare path (default `./data/manga.db`) or `sqlite://` path uses SQLite. `postgres://` and `postgresql://` URLs use PostgreSQL. `config print` reports the backend and omits PostgreSQL credentials and query parameters. `db migrate` applies the matching schema.

SQLite retains WAL and the process write mutex. PostgreSQL uses Toasty's connection pool without that mutex. Its download claim uses an atomic `UPDATE … FOR UPDATE SKIP LOCKED` through Toasty's raw SQL API, since Toasty 0.7 does not expose this lock in model queries. Settings use atomic upserts on both engines. Other application queries use the shared models.

The SQLite upscale queue remains the custom Toasty Apalis backend. PostgreSQL uses `apalis-postgres` 1.0.0-rc.8 with `new_with_notify`, queue `upscale`, one buffered job, and the upstream acknowledgment and abandoned-job recovery middleware. This release combines LISTEN/NOTIFY with a recovery poll so jobs inserted before a listener starts are not lost. Queue migrations live in Apalis's own schema. There is no PostgreSQL fallback to a local SQLite file.

## Initial schemas

This migration is a fresh database boundary. The historical migration chain and archive-index tables are gone. Generate both initial schemas with:

```sh
cargo run -p backend-persistence --example generate-schema
```

Schemas are generated from Toasty 0.7 models and sorted for reproducibility. `constraints.sql` adds composite uniqueness and the partial active-download index, which Toasty 0.7 cannot express. A test checks the committed schemas against generation. Timestamps remain text; adopting `timestamptz` would require a later model migration.

## Tests

`cargo test` uses temporary SQLite files and requires no database service. Set `TEST_POSTGRES_URL` to an isolated PostgreSQL test service with permission to create databases, then run:

```sh
cargo test -p backend-persistence -p manga-server -- --include-ignored
```

Every server database fixture and the chapter regression create a separate temporary database. The explicit backend contract also creates and removes its own database. Use a disposable test cluster to remove the remaining fixture databases after the run. Do not point this setting at a production server. CI runs both passes.

The shared contract covers settings, library upsert and lookup, chapter sync, concurrent download claims, completion, upscale metadata, statistics, read progress, cleanup, and reopening. The server suite also checks native BBF pages, explicit conversions, variants, caches, and durable jobs queued before and after worker startup.

## Dependency packaging

Toasty 0.7's SQLite driver requires `libsqlite3-sys` 0.37. SQLx 0.8.6's optional SQLite driver requires 0.30, and Cargo's single-native-library rule rejects that combination even when SQLx's SQLite feature is off. The two small vendored SQLx facade packages remove that unused dependency from their manifests. PostgreSQL Rust sources are unchanged; each package has a `PATCHES.md`. The application uses SQLx only for PostgreSQL queue integration.
