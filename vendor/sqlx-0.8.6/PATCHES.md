# sqlx 0.8.6 packaging patch

Source: https://github.com/launchbadge/sqlx/tree/v0.8.6 (crates.io release).
Rust sources are unchanged. Cargo.toml omits the unused `sqlx-sqlite` dependency, its feature forwarding, and upstream development targets/dependencies. SQLx is used only for PostgreSQL queues in this application. Toasty 0.7 uses rusqlite 0.39 and libsqlite3-sys 0.37; SQLx 0.8.6's optional SQLite driver pins libsqlite3-sys 0.30, whose `links = sqlite3` conflicts during Cargo resolution even with the SQLite feature disabled.

Both facade and macro-core manifests need this patch. Remove it when a compatible SQLx release is supported by apalis-postgres. Do not enable SQLx SQLite features here; all application SQLite access goes through Toasty/rusqlite.
