use twilight_model::channel::message::Embed;

const MAX_NOTIFICATION_CHAPTERS: usize = 5;

#[derive(Debug)]
pub struct LibraryUpdateNotification {
    pub manga_title: String,
    pub source: String,
    pub cover_url: String,
    pub chapters: Vec<ChapterSummary>,
    pub enqueued_downloads: usize,
}

#[derive(Debug)]
pub struct ChapterSummary {
    pub title: String,
    pub number: f64,
}

#[must_use]
/// Builds the Discord embed used for a library update notification.
pub fn library_update_embed(notification: &LibraryUpdateNotification) -> Embed {
    let description = format!(
        "{} new {} found on `{}`.",
        notification.chapters.len(),
        pluralize(notification.chapters.len(), "chapter", "chapters"),
        notification.source,
    );
    let mut embed = super::embeds::rich(
        &notification.manga_title,
        Some(&description),
        super::embeds::COLOR_SUCCESS,
    );

    let chapters = notification
        .chapters
        .iter()
        .take(MAX_NOTIFICATION_CHAPTERS)
        .map(format_chapter_summary)
        .collect::<Vec<_>>()
        .join("\n");

    embed
        .fields
        .push(super::embeds::field("New chapters", &chapters, false));

    if notification.enqueued_downloads > 0 {
        let downloads_queued = format!(
            "{} {}",
            notification.enqueued_downloads,
            pluralize(notification.enqueued_downloads, "chapter", "chapters"),
        );
        embed.fields.push(super::embeds::field(
            "Downloads queued",
            &downloads_queued,
            true,
        ));
    }

    if notification.chapters.len() > MAX_NOTIFICATION_CHAPTERS {
        embed.footer = Some(super::embeds::footer(&format!(
            "...and {} more.",
            notification.chapters.len() - MAX_NOTIFICATION_CHAPTERS
        )));
    }

    embed.thumbnail = super::embeds::thumbnail(&notification.cover_url);
    embed
}

fn format_chapter_summary(chapter: &ChapterSummary) -> String {
    format!(
        "chapter {}: {}",
        backend_core::format_chapter_number(chapter.number),
        chapter.title
    )
}

fn pluralize(count: usize, singular: &'static str, plural: &'static str) -> &'static str {
    if count == 1 { singular } else { plural }
}
