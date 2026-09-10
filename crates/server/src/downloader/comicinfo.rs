pub(crate) fn comicinfo_age_rating(is_nsfw: bool) -> &'static str {
    if is_nsfw {
        "Adults Only 18+"
    } else {
        "Rating Pending"
    }
}

pub(crate) fn comicinfo_count_for_series(status: &str, total_chapters: usize) -> Option<usize> {
    if total_chapters == 0 {
        return None;
    }

    is_completed_series_status(status).then_some(total_chapters)
}

fn is_completed_series_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed" | "complete" | "finished" | "ended" | "cancelled" | "canceled"
    )
}
