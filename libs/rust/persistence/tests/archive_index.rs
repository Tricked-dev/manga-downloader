use backend_persistence::{ArchiveIndexIdentity, Database};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::test]
async fn archive_index_persists_fresh_rows_and_ignores_stale_identity() {
    let db = temp_database().await;
    let identity = ArchiveIndexIdentity {
        archive_path: "/tmp/archive-a.tar.zst".to_string(),
        archive_size: 1024,
        archive_mtime_ms: 12345,
        schema_version: 1,
    };

    db.upsert_archive_index(&identity, 12, b"postcard-bytes")
        .await
        .expect("archive index row should persist");

    let fresh = db
        .get_fresh_archive_index(&identity)
        .await
        .expect("fresh lookup should succeed")
        .expect("fresh row should load");
    assert_eq!(fresh.identity.archive_path, identity.archive_path);
    assert_eq!(fresh.page_count, 12);
    assert_eq!(fresh.index_postcard, b"postcard-bytes");
    assert!(fresh.last_used_at.is_some());

    let stale_identity = ArchiveIndexIdentity {
        archive_size: 2048,
        ..identity.clone()
    };
    let stale = db
        .get_fresh_archive_index(&stale_identity)
        .await
        .expect("stale lookup should succeed");
    assert!(stale.is_none());

    let stats = db.archive_index_stats().await.expect("stats should load");
    assert_eq!(stats.indexed_archives, 1);
    assert_eq!(stats.indexed_pages, 12);
    assert_eq!(stats.index_blob_bytes, b"postcard-bytes".len());

    let removed = db
        .delete_archive_indexes_by_path(&[identity.archive_path])
        .await
        .expect("delete should succeed");
    assert_eq!(removed.removed_rows, 1);
    assert_eq!(
        db.archive_index_stats()
            .await
            .expect("stats should load")
            .indexed_archives,
        0
    );
}

async fn temp_database() -> Database {
    let path = temp_database_path();
    Database::new(&path.to_string_lossy())
        .await
        .expect("database should initialize")
}

fn temp_database_path() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("manga-persistence-test-{unique}.sqlite3"))
}
