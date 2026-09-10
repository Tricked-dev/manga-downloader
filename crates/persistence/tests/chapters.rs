use backend_persistence::{ChapterInsert, Database, MangaInsert};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::test]
async fn sync_chapters_replaces_stale_remote_id_by_chapter_number() {
    let db = temp_database().await;
    let manga_id = db
        .add_manga_to_library(&MangaInsert {
            source: "comix",
            source_id: "2x32",
            title: "Disastrous Necromancer",
            cover_url: "",
            cover_fetch_spec: None,
            description: "",
            author: "",
            genres: "",
            status: "ongoing",
            category: "default",
            is_nsfw: false,
            language: Some("en"),
        })
        .await
        .expect("manga should be inserted");

    let new_ids = db
        .sync_chapters(
            &manga_id,
            vec![ChapterInsert {
                source_id: "8683654".to_string(),
                title: "Chapter 5".to_string(),
                chapter_number: 5.0,
                date_uploaded: "2026-01-01T00:00:00Z".to_string(),
            }],
        )
        .await
        .expect("initial chapter sync should succeed");
    let original_chapter_id = new_ids
        .first()
        .expect("initial sync should create a chapter")
        .clone();

    let new_ids = db
        .sync_chapters(
            &manga_id,
            vec![ChapterInsert {
                source_id: "https://comix.to/title/2x32-disastrous-necromancer/8683654-chapter-5"
                    .to_string(),
                title: "Chapter 5".to_string(),
                chapter_number: 5.0,
                date_uploaded: "2026-01-01T00:00:00Z".to_string(),
            }],
        )
        .await
        .expect("refreshed chapter sync should succeed");

    assert!(new_ids.is_empty());

    let chapters = db
        .get_chapters(&manga_id)
        .await
        .expect("chapters should load");
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0].id, original_chapter_id);
    assert_eq!(
        chapters[0].source_id,
        "https://comix.to/title/2x32-disastrous-necromancer/8683654-chapter-5"
    );
}

async fn temp_database() -> Database {
    let path = temp_database_path();
    let database_url = if let Ok(url) = std::env::var("TEST_POSTGRES_URL") {
        let pool = sqlx::PgPool::connect(&url)
            .await
            .expect("connect to PostgreSQL test service");
        let name = format!("manga_chapters_test_{}", uuid::Uuid::now_v7().simple());
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&pool)
            .await
            .expect("create isolated test database");
        let mut url = url::Url::parse(&url).expect("valid PostgreSQL URL");
        url.set_path(&name);
        pool.close().await;
        url.to_string()
    } else {
        path.to_string_lossy().into_owned()
    };
    Database::open(&database_url)
        .await
        .expect("database should initialize")
}

fn temp_database_path() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("manga-persistence-chapters-test-{unique}.sqlite3"))
}
