use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use std::path::Path;
use toasty::Db;
use toasty::schema::ModelSet;
use toasty::schema::db::Migration;
use toasty_core::driver::Driver as _;
use toasty_driver_sqlite::Sqlite;

#[derive(Debug, Clone, Copy)]
pub struct DatabaseMigration {
    pub id: u64,
    pub name: &'static str,
    pub sql: &'static str,
}

impl DatabaseMigration {
    #[must_use]
    /// Creates a static migration descriptor.
    pub const fn new(id: u64, name: &'static str, sql: &'static str) -> Self {
        Self { id, name, sql }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SqliteDatabaseOptions {
    pub journal_mode_wal: bool,
    pub foreign_keys: bool,
}

impl Default for SqliteDatabaseOptions {
    fn default() -> Self {
        Self {
            journal_mode_wal: true,
            foreign_keys: false,
        }
    }
}

impl SqliteDatabaseOptions {
    #[must_use]
    /// Returns options with SQLite foreign-key enforcement toggled.
    pub const fn with_foreign_keys(mut self, enabled: bool) -> Self {
        self.foreign_keys = enabled;
        self
    }
}

/// Opens a Toasty SQLite database after ensuring migrations and pragmas are applied.
pub async fn open_sqlite_database(
    path: &str,
    models: ModelSet,
    migrations: &[DatabaseMigration],
    options: SqliteDatabaseOptions,
) -> Result<Db> {
    setup_sqlite_database(path, migrations, options).await?;

    let mut builder = Db::builder();
    builder.models(models);
    builder.build(Sqlite::open(path)).await.map_err(Into::into)
}

/// Prepares a SQLite database file by creating its parent directory and applying migrations.
pub async fn setup_sqlite_database(
    path: &str,
    migrations: &[DatabaseMigration],
    options: SqliteDatabaseOptions,
) -> Result<()> {
    ensure_database_parent_dir(path)?;
    apply_sqlite_migrations(path, migrations).await?;

    let connection = open_configured_sqlite_connection(path, options)?;
    drop(connection);

    Ok(())
}

/// Applies all missing migrations and rejects unknown applied migration ids.
pub async fn apply_sqlite_migrations(path: &str, migrations: &[DatabaseMigration]) -> Result<()> {
    let driver = Sqlite::open(path);
    let mut connection = driver.connect().await?;
    let applied = connection.applied_migrations().await?;

    for applied_migration in &applied {
        if !migrations
            .iter()
            .any(|migration| migration.id == applied_migration.id())
        {
            bail!(
                "database at `{path}` has unknown Toasty migration id {}; this build cannot continue",
                applied_migration.id()
            );
        }
    }

    for migration in migrations {
        if applied.iter().any(|applied| applied.id() == migration.id) {
            continue;
        }

        connection
            .apply_migration(
                migration.id,
                migration.name,
                &Migration::new_sql(migration.sql.trim().to_string()),
            )
            .await?;
    }

    Ok(())
}

/// Opens a rusqlite connection and applies the requested SQLite options.
pub fn open_configured_sqlite_connection(
    path: &str,
    options: SqliteDatabaseOptions,
) -> Result<Connection> {
    let connection =
        Connection::open(path).with_context(|| format!("failed to open SQLite database {path}"))?;
    configure_sqlite_connection(&connection, options)?;
    Ok(connection)
}

/// Applies runtime SQLite pragmas to an existing connection.
pub fn configure_sqlite_connection(
    connection: &Connection,
    options: SqliteDatabaseOptions,
) -> Result<()> {
    if options.journal_mode_wal {
        connection.pragma_update(None, "journal_mode", "WAL")?;
    }
    if options.foreign_keys {
        connection.pragma_update(None, "foreign_keys", "ON")?;
    }
    Ok(())
}

/// Creates the database parent directory when `path` has one.
pub fn ensure_database_parent_dir(path: &str) -> Result<()> {
    let Some(parent) = database_parent_dir(path) else {
        return Ok(());
    };

    backend_fs::create_dir_all_sync(parent)
        .with_context(|| format!("failed to create database directory {}", parent.display()))?;
    Ok(())
}

#[must_use]
/// Returns the meaningful parent directory for a database path.
pub fn database_parent_dir(path: &str) -> Option<&Path> {
    let path = Path::new(path);
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty() && *parent != Path::new("."))
}

/// Open PostgreSQL through the same type-erased Toasty database interface.
pub async fn open_postgres_database(
    url: &str,
    models: ModelSet,
    migrations: &[DatabaseMigration],
) -> Result<Db> {
    apply_postgres_migrations(url, migrations).await?;
    let driver = toasty_driver_postgresql::PostgreSQL::new(url)?;
    let mut builder = Db::builder();
    builder.models(models);
    Ok(builder.build(driver).await?)
}

pub async fn apply_postgres_migrations(url: &str, migrations: &[DatabaseMigration]) -> Result<()> {
    let driver = toasty_driver_postgresql::PostgreSQL::new(url)?;
    let mut connection = driver.connect().await?;
    let applied = connection.applied_migrations().await?;
    for migration in &applied {
        if !migrations.iter().any(|known| known.id == migration.id()) {
            bail!(
                "PostgreSQL database has an unknown migration id {}",
                migration.id()
            );
        }
    }
    for migration in migrations {
        if !applied.iter().any(|applied| applied.id() == migration.id) {
            connection
                .apply_migration(
                    migration.id,
                    migration.name,
                    &Migration::new_sql(migration.sql.trim().to_string()),
                )
                .await?;
        }
    }
    Ok(())
}
