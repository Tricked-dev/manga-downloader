use backend_persistence::{
    ChapterInsert, Database, DatabaseBackend, MangaInsert, generate_schema, migrate_database,
    redacted_database_url,
};

#[test]
fn committed_schemas_match_models() {
    assert_eq!(
        generate_schema(DatabaseBackend::Sqlite).unwrap(),
        include_str!("../migrations/sqlite/0001_initial.sql")
    );
    assert_eq!(
        generate_schema(DatabaseBackend::Postgres).unwrap(),
        include_str!("../migrations/postgres/0001_initial.sql")
    );
}

#[test]
fn database_urls_classify_and_redact_credentials() {
    assert_eq!(
        DatabaseBackend::from_url("data/manga.db"),
        DatabaseBackend::Sqlite
    );
    assert_eq!(
        DatabaseBackend::from_url("postgresql://localhost/manga"),
        DatabaseBackend::Postgres
    );
    assert_eq!(
        redacted_database_url("postgres://alice:secret@localhost:5432/manga?password=secret"),
        "postgres://localhost:5432/manga"
    );
}

#[tokio::test]
async fn sqlite_contract() {
    let path = std::env::temp_dir().join(format!("manga-db-{}.sqlite3", uuid::Uuid::now_v7()));
    database_contract(
        &format!("sqlite://{}", path.display()),
        DatabaseBackend::Sqlite,
    )
    .await;
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
#[ignore = "requires TEST_POSTGRES_URL with permission to create a temporary database"]
async fn postgres_contract() {
    let url = std::env::var("TEST_POSTGRES_URL").expect("TEST_POSTGRES_URL is required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    let name = format!("manga_test_{}", uuid::Uuid::now_v7().simple());
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&pool)
        .await
        .unwrap();
    let mut test_url = url::Url::parse(&url).unwrap();
    test_url.set_path(&name);
    database_contract(test_url.as_str(), DatabaseBackend::Postgres).await;
    sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

async fn database_contract(url: &str, backend: DatabaseBackend) {
    migrate_database(url).await.unwrap();
    let db = Database::open(url).await.unwrap();
    assert_eq!(db.backend(), backend);
    db.ping().await.unwrap();
    assert_eq!(
        db.get_setting("auto_upscale").await.unwrap().as_deref(),
        Some("true")
    );
    db.set_setting("contract", "one").await.unwrap();
    db.set_setting("contract", "two").await.unwrap();
    assert_eq!(
        db.get_setting("contract").await.unwrap().as_deref(),
        Some("two")
    );
    let manga = MangaInsert {
        source: "comix",
        source_id: "contract-series",
        title: "Contract series",
        cover_url: "",
        cover_fetch_spec: None,
        description: "A series",
        author: "Author",
        genres: "action, adventure",
        status: "ongoing",
        category: "default",
        is_nsfw: false,
        language: Some("en"),
    };
    let series = db.add_manga_to_library(&manga).await.unwrap();
    assert_eq!(series, db.add_manga_to_library(&manga).await.unwrap());
    assert_eq!(db.get_library_manga().await.unwrap().len(), 1);
    assert_eq!(
        db.get_manga_by_source("comix", "contract-series")
            .await
            .unwrap()
            .unwrap()
            .id,
        series
    );
    let chapters = db
        .sync_chapters(
            &series,
            vec![ChapterInsert {
                source_id: "chapter-1".into(),
                title: "Chapter 1".into(),
                chapter_number: 1.5,
                date_uploaded: "2026-01-01".into(),
            }],
        )
        .await
        .unwrap();
    let chapter = &chapters[0];
    assert_eq!(
        db.get_chapters(&series).await.unwrap()[0].chapter_number,
        1.5
    );
    let download = db.enqueue_download(chapter, &series).await.unwrap();
    assert_eq!(
        download,
        db.enqueue_download(chapter, &series).await.unwrap()
    );
    let (first, second) =
        tokio::join!(db.get_next_queued_download(), db.get_next_queued_download());
    let claimed = [first.unwrap(), second.unwrap()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(
        claimed.len(),
        1,
        "a queued download may only be claimed once"
    );
    assert_eq!(claimed[0].id, download);
    assert!(db.get_next_queued_download().await.unwrap().is_none());
    db.complete_download(&download, 3, "/fixture/chapter.bbf", 1024)
        .await
        .unwrap();
    let before = db.get_stats_overview().await.unwrap();
    assert_eq!(before.totals.pages_downloaded, 3);
    assert_eq!(before.totals.chapters_downloaded, 1);
    db.mark_upscaled(&download, "model.onnx", 2, 2048)
        .await
        .unwrap();
    let row = db.get_download_by_id(&download).await.unwrap().unwrap();
    assert!(row.upscaled_at.is_some());
    assert_eq!(row.upscale_model.as_deref(), Some("model.onnx"));
    assert_eq!(row.upscale_scale, Some(2));
    let after = db.get_stats_overview().await.unwrap();
    assert_eq!(
        after.totals.pages_downloaded,
        before.totals.pages_downloaded
    );
    assert_eq!(
        after
            .activity
            .iter()
            .map(|day| day.chapters_downloaded)
            .sum::<usize>(),
        1
    );
    db.update_chapter_read_progress(chapter, 2, false)
        .await
        .unwrap();
    assert_eq!(
        db.get_chapter_by_id(chapter)
            .await
            .unwrap()
            .unwrap()
            .pages_read,
        2
    );
    db.cleanup_database().await.unwrap();
    drop(db);
    // Reopening verifies migrations are idempotent and metadata survives restart.
    let db = Database::open(url).await.unwrap();
    assert_eq!(
        db.get_download_by_id(&download)
            .await
            .unwrap()
            .unwrap()
            .upscale_scale,
        Some(2)
    );
    db.delete_download_and_reset_chapter(&download, chapter)
        .await
        .unwrap();
    db.remove_from_library(&series).await.unwrap();
    assert!(db.get_library_manga().await.unwrap().is_empty());
    assert!(db.get_all_library_chapters().await.unwrap().is_empty());
}
