use backend_discord::{ChapterSummary, LibraryUpdateNotification, library_update_embed};

#[test]
fn library_update_embed_summarizes_chapters_downloads_and_thumbnail() {
    let notification = LibraryUpdateNotification {
        manga_title: "A Very Good Series".to_string(),
        source: "ExampleSource".to_string(),
        cover_url: " https://cdn.example.test/cover.jpg ".to_string(),
        chapters: (1..=6)
            .map(|number| ChapterSummary {
                title: format!("Chapter {number}"),
                number: f64::from(number),
            })
            .collect(),
        enqueued_downloads: 2,
    };

    let embed = library_update_embed(&notification);

    assert_eq!(embed.title.as_deref(), Some("A Very Good Series"));
    assert_eq!(
        embed.description.as_deref(),
        Some("6 new chapters found on `ExampleSource`.")
    );
    assert_eq!(embed.fields.len(), 2);
    assert_eq!(embed.fields[0].name, "New chapters");
    assert!(embed.fields[0].value.contains("chapter 1: Chapter 1"));
    assert!(embed.fields[0].value.contains("chapter 5: Chapter 5"));
    assert!(!embed.fields[0].value.contains("chapter 6: Chapter 6"));
    assert_eq!(embed.fields[1].name, "Downloads queued");
    assert_eq!(embed.fields[1].value, "2 chapters");
    assert_eq!(
        embed.footer.expect("overflow footer should exist").text,
        "...and 1 more."
    );
    assert_eq!(
        embed.thumbnail.expect("thumbnail should be set").url,
        "https://cdn.example.test/cover.jpg"
    );
}

#[test]
fn library_update_embed_omits_invalid_thumbnail_and_download_field() {
    let notification = LibraryUpdateNotification {
        manga_title: "Series".to_string(),
        source: "ExampleSource".to_string(),
        cover_url: "file:///tmp/cover.jpg".to_string(),
        chapters: vec![ChapterSummary {
            title: "Only Chapter".to_string(),
            number: 1.5,
        }],
        enqueued_downloads: 0,
    };

    let embed = library_update_embed(&notification);

    assert_eq!(
        embed.description.as_deref(),
        Some("1 new chapter found on `ExampleSource`.")
    );
    assert_eq!(embed.fields.len(), 1);
    assert_eq!(embed.fields[0].value, "chapter 1.5: Only Chapter");
    assert!(embed.thumbnail.is_none());
    assert!(embed.footer.is_none());
}
