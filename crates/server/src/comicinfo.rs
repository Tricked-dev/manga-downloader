use anyhow::{Context, Result};
use backend_persistence::MangaRow;
use serde::Deserialize;
use std::collections::BTreeSet;

const ANILIST_GRAPHQL_URL: &str = "https://graphql.anilist.co";
const ANILIST_MEDIA_QUERY: &str = r"
query ($id: Int, $search: String) {
  Media(id: $id, search: $search, type: MANGA) {
    id
    title {
      romaji
      english
      native
    }
    synonyms
    externalLinks {
      site
      url
    }
    staff(sort: [RELEVANCE, ID]) {
      edges {
        role
        node {
          name {
            full
            native
          }
        }
      }
    }
  }
}
";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ComicInfoCredits {
    pub writer: Option<String>,
    pub artist: Option<String>,
    pub penciller: Option<String>,
    pub inker: Option<String>,
    pub letterer: Option<String>,
    pub editor: Option<String>,
    pub publisher: Option<String>,
    pub imprint: Option<String>,
}

impl ComicInfoCredits {
    #[must_use]
    pub fn from_author_fallback(author: &str) -> Self {
        let author = author.trim();
        if author.is_empty() {
            Self::default()
        } else {
            Self {
                writer: Some(author.to_string()),
                artist: Some(author.to_string()),
                ..Self::default()
            }
        }
    }

    pub fn overlay(&mut self, other: Self) {
        overlay_field(&mut self.writer, other.writer);
        overlay_field(&mut self.artist, other.artist);
        overlay_field(&mut self.penciller, other.penciller);
        overlay_field(&mut self.inker, other.inker);
        overlay_field(&mut self.letterer, other.letterer);
        overlay_field(&mut self.editor, other.editor);
        overlay_field(&mut self.publisher, other.publisher);
        overlay_field(&mut self.imprint, other.imprint);
    }

    #[must_use]
    pub fn as_borrowed(&self) -> backend_core::ComicInfoCredits<'_> {
        backend_core::ComicInfoCredits {
            writer: self.writer.as_deref(),
            artist: self.artist.as_deref(),
            penciller: self.penciller.as_deref(),
            inker: self.inker.as_deref(),
            letterer: self.letterer.as_deref(),
            editor: self.editor.as_deref(),
            publisher: self.publisher.as_deref(),
            imprint: self.imprint.as_deref(),
        }
    }
}

pub async fn resolve_comicinfo_credits(manga: &MangaRow) -> ComicInfoCredits {
    let mut credits = ComicInfoCredits::from_author_fallback(&manga.author);

    match fetch_remote_comicinfo_credits(manga).await {
        Ok(Some(remote_credits)) => credits.overlay(remote_credits),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                manga_id = %manga.id,
                source = %manga.source,
                source_id = %manga.source_id,
                error = %error,
                "ComicInfo Credits Enrichment Failed",
            );
        }
    }

    credits
}

async fn fetch_remote_comicinfo_credits(manga: &MangaRow) -> Result<Option<ComicInfoCredits>> {
    backend_tls::ensure_graviola_rustls_provider()?;
    let client = reqwest::Client::builder()
        .user_agent("manga-downloader-comicinfo")
        .build()
        .context("failed to build ComicInfo HTTP client")?;
    let title_candidates = vec![manga.title.clone()];
    let media = search_anilist_media(&client, &title_candidates).await?;

    Ok(media.map(map_anilist_media_to_credits))
}

async fn search_anilist_media(
    client: &reqwest::Client,
    title_candidates: &[String],
) -> Result<Option<AniListMedia>> {
    let queries = normalized_nonempty_titles(title_candidates);

    for query in &queries {
        let Some(media) = fetch_anilist_media(client, None, Some(query)).await? else {
            continue;
        };

        if media_matches_any_title(&media, &queries) {
            return Ok(Some(media));
        }
    }

    Ok(None)
}

async fn fetch_anilist_media(
    client: &reqwest::Client,
    id: Option<i32>,
    search: Option<&str>,
) -> Result<Option<AniListMedia>> {
    let response = client
        .post(ANILIST_GRAPHQL_URL)
        .json(&serde_json::json!({
            "query": ANILIST_MEDIA_QUERY,
            "variables": {
                "id": id,
                "search": search,
            },
        }))
        .send()
        .await
        .context("AniList request failed")?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read AniList response")?;

    if !status.is_success() {
        if status == reqwest::StatusCode::NOT_FOUND
            && let Ok(payload) = serde_json::from_str::<GraphqlResponse<AniListMediaData>>(&body)
            && payload.is_not_found()
        {
            return Ok(None);
        }

        let body_excerpt = body.chars().take(500).collect::<String>();
        anyhow::bail!("AniList request returned HTTP {status}: {body_excerpt}");
    }

    let payload = serde_json::from_str::<GraphqlResponse<AniListMediaData>>(&body)
        .context("failed to parse AniList response")?;

    if let Some(errors) = payload.errors
        && !errors.is_empty()
    {
        let messages = errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("; ");
        anyhow::bail!("AniList query failed: {messages}");
    }

    Ok(payload.data.and_then(|data| data.media))
}

fn push_unique_title(titles: &mut Vec<String>, title: String) {
    if title.trim().is_empty() {
        return;
    }

    if !titles.iter().any(|existing| existing == &title) {
        titles.push(title);
    }
}

fn extend_unique(titles: &mut Vec<String>, more_titles: Vec<String>) {
    for title in more_titles {
        push_unique_title(titles, title);
    }
}

fn normalized_nonempty_titles(titles: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for title in titles {
        let title = title.trim();
        if title.is_empty() {
            continue;
        }
        if !normalized.iter().any(|existing| existing == title) {
            normalized.push(title.to_string());
        }
    }
    normalized
}

fn media_matches_any_title(media: &AniListMedia, title_candidates: &[String]) -> bool {
    let media_titles = media
        .all_titles()
        .into_iter()
        .map(|title| normalize_title(&title))
        .filter(|title| !title.is_empty())
        .collect::<BTreeSet<_>>();

    title_candidates
        .iter()
        .map(|title| normalize_title(title))
        .filter(|title| !title.is_empty())
        .any(|title| media_titles.contains(&title))
}

fn normalize_title(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn map_anilist_media_to_credits(media: AniListMedia) -> ComicInfoCredits {
    let mut writers = BTreeSet::new();
    let mut artists = BTreeSet::new();
    let mut pencillers = BTreeSet::new();
    let mut inkers = BTreeSet::new();
    let mut letterers = BTreeSet::new();
    let mut editors = BTreeSet::new();

    if let Some(staff) = media.staff {
        for edge in staff.edges {
            let name = preferred_staff_name(&edge.node.name);
            if name.is_empty() {
                continue;
            }

            let role = edge.role.to_ascii_lowercase();
            if is_writer_role(&role) {
                writers.insert(name.clone());
            }
            if is_artist_role(&role) {
                artists.insert(name.clone());
            }
            if role.contains("penc") {
                pencillers.insert(name.clone());
            }
            if role.contains("ink") {
                inkers.insert(name.clone());
            }
            if role.contains("letter") {
                letterers.insert(name.clone());
            }
            if role.contains("edit") {
                editors.insert(name);
            }
        }
    }

    ComicInfoCredits {
        writer: join_names(writers),
        artist: join_names(artists),
        penciller: join_names(pencillers),
        inker: join_names(inkers),
        letterer: join_names(letterers),
        editor: join_names(editors),
        publisher: publisher_from_external_links(&media.external_links),
        imprint: None,
    }
}

fn overlay_field(slot: &mut Option<String>, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        *slot = Some(value);
    }
}

fn preferred_staff_name(name: &AniListName) -> String {
    name.full
        .as_deref()
        .or(name.native.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn join_names(names: BTreeSet<String>) -> Option<String> {
    if names.is_empty() {
        None
    } else {
        Some(names.into_iter().collect::<Vec<_>>().join(", "))
    }
}

fn is_writer_role(role: &str) -> bool {
    role.contains("story")
        || role.contains("writer")
        || role.contains("script")
        || role.contains("scenario")
        || role.contains("adapt")
        || role.contains("composition")
        || role.contains("original creator")
        || role.contains("original work")
}

fn is_artist_role(role: &str) -> bool {
    role.contains("art")
        || role.contains("artist")
        || role.contains("illustrat")
        || role.contains("draw")
        || role.contains("character design")
}

fn publisher_from_external_links(links: &[AniListExternalLink]) -> Option<String> {
    links
        .iter()
        .map(|link| link.site.trim())
        .find(|site| {
            let normalized = site.to_ascii_lowercase();
            normalized.contains("press")
                || normalized.contains("media")
                || normalized.contains("books")
                || normalized.contains("comics")
                || normalized.contains("publishing")
                || normalized.contains("novel club")
                || normalized.contains("entertainment")
        })
        .map(ToString::to_string)
}

#[derive(Debug, Deserialize)]
struct GraphqlResponse<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Option<Vec<GraphqlError>>,
}

impl<T> GraphqlResponse<T> {
    fn is_not_found(&self) -> bool {
        self.errors.as_ref().is_some_and(|errors| {
            errors.iter().any(|error| {
                error.status == Some(404) || error.message.eq_ignore_ascii_case("not found.")
            })
        })
    }
}

#[derive(Debug, Deserialize)]
struct GraphqlError {
    message: String,
    status: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct AniListMediaData {
    #[serde(rename = "Media")]
    media: Option<AniListMedia>,
}

#[derive(Debug, Deserialize)]
struct AniListMedia {
    title: AniListTitle,
    #[serde(default)]
    synonyms: Vec<String>,
    #[serde(default, rename = "externalLinks")]
    external_links: Vec<AniListExternalLink>,
    staff: Option<AniListStaffConnection>,
}

impl AniListMedia {
    fn all_titles(&self) -> Vec<String> {
        let mut titles = Vec::new();
        for title in [
            self.title.romaji.as_deref(),
            self.title.english.as_deref(),
            self.title.native.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            push_unique_title(&mut titles, title.to_string());
        }
        extend_unique(&mut titles, self.synonyms.clone());
        titles
    }
}

#[derive(Debug, Deserialize)]
struct AniListTitle {
    romaji: Option<String>,
    english: Option<String>,
    native: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AniListExternalLink {
    site: String,
}

#[derive(Debug, Deserialize)]
struct AniListStaffConnection {
    #[serde(default)]
    edges: Vec<AniListStaffEdge>,
}

#[derive(Debug, Deserialize)]
struct AniListStaffEdge {
    role: String,
    node: AniListStaffNode,
}

#[derive(Debug, Deserialize)]
struct AniListStaffNode {
    name: AniListName,
}

#[derive(Debug, Deserialize)]
struct AniListName {
    full: Option<String>,
    native: Option<String>,
}
