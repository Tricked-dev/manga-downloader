fn main() -> anyhow::Result<()> {
    for (directory, backend) in [
        ("sqlite", backend_persistence::DatabaseBackend::Sqlite),
        ("postgres", backend_persistence::DatabaseBackend::Postgres),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("migrations")
            .join(directory)
            .join("0001_initial.sql");
        std::fs::write(path, backend_persistence::generate_schema(backend)?)?;
    }
    Ok(())
}
