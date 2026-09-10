use crate::models::ComixChapter;
use std::collections::HashMap;

const LATEST_CHAPTER_TOLERANCE: f32 = 2.0;

pub fn upsert_chapter(chapter_map: &mut HashMap<String, ComixChapter>, ch: ComixChapter) {
    let key = ch.number.round().to_string();
    if let Some(existing) = chapter_map.get(&key) {
        if is_better(&ch, existing) {
            chapter_map.insert(key, ch);
        }
    } else {
        chapter_map.insert(key, ch);
    }
}

pub fn retain_plausible_chapters(
    chapter_map: &mut HashMap<String, ComixChapter>,
    latest_chapter: Option<f32>,
) {
    let Some(max_allowed) = latest_chapter
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value + LATEST_CHAPTER_TOLERANCE)
    else {
        return;
    };

    chapter_map.retain(|_, chapter| {
        chapter.number.is_finite() && chapter.number > 0.0 && chapter.number <= max_allowed
    });
}

fn is_official_like(ch: &ComixChapter) -> bool {
    ch.group_id() == Some(10702) || ch.is_official_bool()
}

fn is_better(new_ch: &ComixChapter, cur: &ComixChapter) -> bool {
    let official_new = is_official_like(new_ch);
    let official_cur = is_official_like(cur);

    if official_new && !official_cur {
        return true;
    }
    if !official_new && official_cur {
        return false;
    }

    if new_ch.votes.unwrap_or(0) > cur.votes.unwrap_or(0) {
        return true;
    }
    if new_ch.votes.unwrap_or(0) < cur.votes.unwrap_or(0) {
        return false;
    }

    new_ch.chapter_id > cur.chapter_id
}
