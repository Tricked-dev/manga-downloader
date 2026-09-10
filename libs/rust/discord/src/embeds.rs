use twilight_model::channel::message::{
    Embed,
    embed::{EmbedField, EmbedFooter, EmbedThumbnail},
};

pub const COLOR_INFO: u32 = 0x58_65_F2;
pub const COLOR_SUCCESS: u32 = 0x3B_A5_5D;
pub const COLOR_WARNING: u32 = 0xFA_A6_1A;
pub const COLOR_ERROR: u32 = 0xED_42_45;

const EMBED_TITLE_MAX_CHARS: usize = 256;
const EMBED_DESCRIPTION_MAX_CHARS: usize = 4_096;
const EMBED_FIELD_NAME_MAX_CHARS: usize = 256;
const EMBED_FIELD_VALUE_MAX_CHARS: usize = 1_024;
const EMBED_FOOTER_MAX_CHARS: usize = 2_048;

#[must_use]
/// Builds a standard Discord rich embed with truncated title and description.
pub fn rich(title: &str, description: Option<&str>, color: u32) -> Embed {
    Embed {
        author: None,
        color: Some(color),
        description: description
            .map(|description| truncate(description, EMBED_DESCRIPTION_MAX_CHARS)),
        fields: Vec::new(),
        footer: None,
        image: None,
        kind: "rich".to_owned(),
        provider: None,
        thumbnail: None,
        timestamp: None,
        title: Some(truncate(title, EMBED_TITLE_MAX_CHARS)),
        url: None,
        video: None,
    }
}

#[must_use]
/// Builds a Discord embed field with length limits applied.
pub fn field(name: &str, value: &str, inline: bool) -> EmbedField {
    EmbedField {
        inline,
        name: truncate(name, EMBED_FIELD_NAME_MAX_CHARS),
        value: truncate(value, EMBED_FIELD_VALUE_MAX_CHARS),
    }
}

#[must_use]
/// Builds a Discord embed footer with length limits applied.
pub fn footer(text: &str) -> EmbedFooter {
    EmbedFooter {
        icon_url: None,
        proxy_icon_url: None,
        text: truncate(text, EMBED_FOOTER_MAX_CHARS),
    }
}

#[must_use]
/// Builds a Discord embed thumbnail for a valid HTTP(S) image URL.
pub fn thumbnail(url: &str) -> Option<EmbedThumbnail> {
    let url = image_url(url)?;
    Some(EmbedThumbnail {
        height: None,
        proxy_url: None,
        url,
        width: None,
    })
}

fn image_url(value: &str) -> Option<String> {
    let value = value.trim();
    let url = url::Url::parse(value).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| value.to_owned())
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    let keep = max_chars.saturating_sub(3);
    format!("{}...", value.chars().take(keep).collect::<String>())
}
