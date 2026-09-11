use crate::DatabaseMigration;
pub(crate) const SQLITE_MIGRATIONS: &[DatabaseMigration] = &[
    DatabaseMigration::new(
        1,
        "initial_schema",
        include_str!("../migrations/sqlite/0001_initial.sql"),
    ),
    DatabaseMigration::new(
        2,
        "upscale_progress",
        include_str!("../migrations/sqlite/0002_upscale_progress.sql"),
    ),
    DatabaseMigration::new(
        3,
        "api_tokens",
        include_str!("../migrations/sqlite/0003_api_tokens.sql"),
    ),
];
pub(crate) const POSTGRES_MIGRATIONS: &[DatabaseMigration] = &[
    DatabaseMigration::new(
        1,
        "initial_schema",
        include_str!("../migrations/postgres/0001_initial.sql"),
    ),
    DatabaseMigration::new(
        2,
        "upscale_progress",
        include_str!("../migrations/postgres/0002_upscale_progress.sql"),
    ),
    DatabaseMigration::new(
        3,
        "api_tokens",
        include_str!("../migrations/postgres/0003_api_tokens.sql"),
    ),
];
