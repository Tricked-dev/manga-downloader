use std::time::{Duration, Instant};

use crate::api::{
    dto::{
        ApiListResponse, ChapterResponse, MangaResponse, SearchResponse, SourceSettingsResponse,
    },
    error::AppError,
};
use autometrics::autometrics;
use backend_cache::{CacheKey, MangaCache};
use backend_persistence::{Database, SourceRecordInput};
use backend_runtime::truncate_for_log;
use backend_sources::{
    SourceInfo, SourceRegistry,
    media::{encode_media_spec, media_ref_to_spec},
    types,
};
use backend_telemetry::{Metrics, trace};
use tokio::sync::RwLock;
use tracing::Instrument as _;

const SEARCH_QUERY_LOG_MAX_CHARS: usize = 160;
const SEARCH_CACHE_TTL: Duration = Duration::from_hours(1);
const SOURCE_LIST_PLUGIN_LOCK_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone, Copy)]
enum SourceCatalogReadKind {
    Search,
    MangaDetails,
    ChapterList,
}

impl SourceCatalogReadKind {
    const fn metric_label(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::MangaDetails => "manga_details",
            Self::ChapterList => "chapter_list",
        }
    }

    const fn cache_span_name(self) -> &'static str {
        match self {
            Self::Search => "source_search",
            Self::MangaDetails => "source_manga_details",
            Self::ChapterList => "source_chapter_list",
        }
    }
}

struct SourceCatalogRead<'a> {
    source: &'a str,
    kind: SourceCatalogReadKind,
    metrics: &'a Metrics,
    started_at: Instant,
    span: tracing::Span,
}

impl<'a> SourceCatalogRead<'a> {
    fn current(
        source: &'a str,
        kind: SourceCatalogReadKind,
        metrics: &'a Metrics,
        started_at: Instant,
    ) -> Self {
        Self {
            source,
            kind,
            metrics,
            started_at,
            span: tracing::Span::current(),
        }
    }

    fn cache_lookup_span(&self) -> tracing::Span {
        trace::cache_lookup_span("manga_response", self.kind.cache_span_name())
    }

    fn cache_write_span(&self) -> tracing::Span {
        trace::cache_write_span("manga_response", self.kind.cache_span_name())
    }

    fn plugin_span(&self) -> tracing::Span {
        trace::plugin_operation_span(self.source, self.kind.metric_label())
    }

    fn record_cache_hit(&self, cache_span: &tracing::Span, item_count: usize) {
        trace::record_result(cache_span, "hit");
        self.record_success("hit", item_count);
    }

    fn record_cache_miss(&self, cache_span: &tracing::Span) {
        trace::record_result(cache_span, "miss");
    }

    fn record_success_miss(&self, item_count: usize) {
        self.record_success("miss", item_count);
    }

    fn record_success(&self, cache_result: &'static str, item_count: usize) {
        trace::record_outcome(&self.span, "success");
        trace::record_result(&self.span, cache_result);
        trace::record_item_count(&self.span, item_count);
        trace::record_duration(&self.span, self.started_at.elapsed());
        self.metrics.record_source_operation(
            self.source,
            self.kind.metric_label(),
            "success",
            cache_result,
            self.started_at.elapsed(),
            Some(item_count),
        );
    }

    fn record_plugin_success(&self, plugin_span: &tracing::Span, item_count: usize) {
        trace::record_outcome(plugin_span, "success");
        trace::record_item_count(plugin_span, item_count);
    }

    fn record_plugin_error<E: std::fmt::Display + ?Sized>(
        &self,
        plugin_span: &tracing::Span,
        error: &E,
    ) {
        trace::record_error(plugin_span, error);
        self.record_error(error);
    }

    fn record_error<E: std::fmt::Display + ?Sized>(&self, error: &E) {
        trace::record_error(&self.span, error);
        trace::record_result(&self.span, "miss");
        trace::record_duration(&self.span, self.started_at.elapsed());
        self.metrics.record_source_operation(
            self.source,
            self.kind.metric_label(),
            "error",
            "miss",
            self.started_at.elapsed(),
            None,
        );
    }
}

#[autometrics]
#[tracing::instrument(name = "app.sources.list", skip_all, fields(outcome = tracing::field::Empty, item_count = tracing::field::Empty))]
pub async fn list_sources(
    db: &Database,
    source_registry: &RwLock<SourceRegistry>,
) -> Result<ApiListResponse<SourceInfo>, AppError> {
    let sources =
        match tokio::time::timeout(SOURCE_LIST_PLUGIN_LOCK_TIMEOUT, source_registry.read()).await {
            Ok(pm) => pm.sources(),
            Err(_) => {
                tracing::warn!(
                    timeout_ms = SOURCE_LIST_PLUGIN_LOCK_TIMEOUT.as_millis(),
                    "Source List Falling Back To Persisted Metadata",
                );
                db.list_sources()
                    .await?
                    .into_iter()
                    .map(source_info_from_record)
                    .collect()
            }
        };
    let response = ApiListResponse::new(sources);
    let span = tracing::Span::current();
    trace::record_outcome(&span, "success");
    trace::record_item_count(&span, response.items.len());
    Ok(response)
}

fn source_info_from_record(source: SourceRecordInput) -> SourceInfo {
    SourceInfo {
        name: source.key,
        display_name: source.display_name,
        base_url: source.base_url,
        plugin_version: source.version,
        plugin_api_version: source.plugin_api_version,
        capabilities: source.capabilities,
        search_categories: Vec::new(),
        default_search_category: None,
        supports_search_popularity: false,
        homepage: None,
        source_repository: None,
        build_metadata: None,
        enabled: source.enabled,
    }
}

#[autometrics]
#[tracing::instrument(name = "app.sources.settings.get", skip_all, fields(source = %name, outcome = tracing::field::Empty))]
pub async fn get_source_settings(
    db: &Database,
    source_registry: &RwLock<SourceRegistry>,
    name: &str,
) -> Result<SourceSettingsResponse, AppError> {
    let pm = source_registry.read().await;
    pm.source_base_url(name)?;
    drop(pm);

    let db_span = trace::db_operation_span("get_source_hide_nsfw", "source_settings");
    let db_started_at = Instant::now();
    let hide_nsfw = async { db.get_source_hide_nsfw(name).await }
        .instrument(db_span.clone())
        .await?;
    trace::record_outcome(&db_span, "success");
    trace::record_duration(&db_span, db_started_at.elapsed());

    trace::record_outcome(&tracing::Span::current(), "success");
    Ok(SourceSettingsResponse {
        auto_upscale: super::settings::source_auto_upscale(db, name).await?,
        name: name.to_owned(),
        hide_nsfw,
    })
}

pub struct SearchSourceInput {
    pub name: String,
    pub query: String,
    pub page: u32,
    pub category: Option<String>,
    pub popular: bool,
}

#[autometrics(track_concurrency)]
#[tracing::instrument(
    name = "app.source.search",
    skip_all,
    fields(
        source = tracing::field::Empty,
        page = tracing::field::Empty,
        category = tracing::field::Empty,
        popular = tracing::field::Empty,
        query_present = tracing::field::Empty,
        outcome = tracing::field::Empty,
        cache_result = tracing::field::Empty,
        item_count = tracing::field::Empty,
    )
)]
pub async fn search_source(
    db: &Database,
    source_registry: &RwLock<SourceRegistry>,
    cache: &MangaCache,
    metrics: &Metrics,
    input: SearchSourceInput,
) -> Result<SearchResponse, AppError> {
    let SearchSourceInput {
        name,
        query,
        page,
        category,
        popular,
    } = input;
    let span = tracing::Span::current();
    span.record("source", tracing::field::display(&name));
    span.record("page", page);
    span.record(
        "category",
        tracing::field::display(category.as_deref().unwrap_or("")),
    );
    span.record("popular", popular);
    span.record("query_present", !query.trim().is_empty());
    let started_at = Instant::now();
    let catalog_read =
        SourceCatalogRead::current(&name, SourceCatalogReadKind::Search, metrics, started_at);
    let db_span = trace::db_operation_span("get_source_hide_nsfw", "source_settings");
    let db_started_at = Instant::now();
    let hide_nsfw = match async { db.get_source_hide_nsfw(&name).await }
        .instrument(db_span.clone())
        .await
    {
        Ok(hide_nsfw) => {
            trace::record_outcome(&db_span, "success");
            trace::record_duration(&db_span, db_started_at.elapsed());
            hide_nsfw
        }
        Err(error) => {
            trace::record_error(&db_span, &error);
            trace::record_duration(&db_span, db_started_at.elapsed());
            trace::record_error(&span, &error);
            trace::record_duration(&span, started_at.elapsed());
            return Err(AppError::from(error));
        }
    };
    let log_context = SourceSearchLogContext {
        source: &name,
        query: truncate_for_log(&query, SEARCH_QUERY_LOG_MAX_CHARS),
        page,
        category: category.as_deref().unwrap_or(""),
        popular,
        hide_nsfw,
        should_log: page == 1 && !query.trim().is_empty(),
        started_at,
    };
    let cache_key = CacheKey::Search {
        plugin: name.clone(),
        query: query.clone(),
        page,
        category: category.clone(),
        popular,
        hide_nsfw,
    };
    let cache_span = catalog_read.cache_lookup_span();
    let cache_started_at = Instant::now();
    let cached = async { cache.get_typed::<SearchResponse>(&cache_key).await }
        .instrument(cache_span.clone())
        .await;
    trace::record_duration(&cache_span, cache_started_at.elapsed());
    if let Some(cached) = cached {
        log_context.success(true, &cached);
        catalog_read.record_cache_hit(&cache_span, cached.mangas.len());
        return Ok(cached);
    }
    catalog_read.record_cache_miss(&cache_span);

    let pm = source_registry.read().await;
    let source_base_url = match pm.source_base_url(&name) {
        Ok(source_base_url) => source_base_url,
        Err(error) => {
            log_context.error(false, &error);
            catalog_read.record_error(&error);
            return Err(AppError::from(error));
        }
    };
    let plugin_span = catalog_read.plugin_span();
    let plugin_started_at = Instant::now();
    let result = pm
        .search_manga(&name, &query, page, category.as_deref(), popular)
        .instrument(plugin_span.clone())
        .await;
    trace::record_duration(&plugin_span, plugin_started_at.elapsed());
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            log_context.error(false, &error);
            catalog_read.record_plugin_error(&plugin_span, &error);
            return Err(AppError::from(error));
        }
    };
    catalog_read.record_plugin_success(&plugin_span, result.items.len());

    let mangas = match result
        .items
        .into_iter()
        .filter(|manga| !hide_nsfw || !manga.is_nsfw)
        .map(|manga| map_manga_response(manga, &source_base_url, &name))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(mangas) => mangas,
        Err(error) => {
            log_context.error(false, &error);
            catalog_read.record_error(&error);
            return Err(error);
        }
    };

    let response = SearchResponse {
        mangas,
        has_next_page: result.has_next_page,
    };

    let cache_write_span = catalog_read.cache_write_span();
    let cache_write_started_at = Instant::now();
    {
        let _entered = cache_write_span.enter();
        cache.insert_typed_ttl(cache_key, &response, SEARCH_CACHE_TTL);
    }
    trace::record_duration(&cache_write_span, cache_write_started_at.elapsed());
    log_context.success(false, &response);
    catalog_read.record_success_miss(response.mangas.len());
    Ok(response)
}

struct SourceSearchLogContext<'a> {
    source: &'a str,
    query: String,
    page: u32,
    category: &'a str,
    popular: bool,
    hide_nsfw: bool,
    should_log: bool,
    started_at: Instant,
}

impl SourceSearchLogContext<'_> {
    fn success(&self, cache_hit: bool, response: &SearchResponse) {
        if !self.should_log {
            return;
        }

        let correlation = trace::current_trace_context();
        tracing::info!(
            trace_id = correlation.trace_id(),
            span_id = correlation.span_id(),
            source = %self.source,
            query = %self.query,
            page = self.page,
            category = %self.category,
            popular = self.popular,
            hide_nsfw = self.hide_nsfw,
            cache_hit,
            result_count = response.mangas.len(),
            has_next_page = response.has_next_page,
            duration_ms = self.started_at.elapsed().as_millis(),
            outcome = "success",
            "Source Search Executed",
        );
    }

    fn error<E: std::fmt::Display + ?Sized>(&self, cache_hit: bool, error: &E) {
        if !self.should_log {
            return;
        }

        let correlation = trace::current_trace_context();
        tracing::info!(
            trace_id = correlation.trace_id(),
            span_id = correlation.span_id(),
            source = %self.source,
            query = %self.query,
            page = self.page,
            category = %self.category,
            popular = self.popular,
            hide_nsfw = self.hide_nsfw,
            cache_hit,
            error = %error,
            duration_ms = self.started_at.elapsed().as_millis(),
            outcome = "error",
            "Source Search Executed",
        );
    }
}

#[autometrics]
#[tracing::instrument(name = "app.source.manga_details", skip_all, fields(source = %name, remote_id = %id, outcome = tracing::field::Empty, cache_result = tracing::field::Empty, item_count = tracing::field::Empty))]
pub async fn get_manga_details(
    source_registry: &RwLock<SourceRegistry>,
    cache: &MangaCache,
    metrics: &Metrics,
    name: String,
    id: String,
) -> Result<MangaResponse, AppError> {
    let started_at = Instant::now();
    let catalog_read = SourceCatalogRead::current(
        &name,
        SourceCatalogReadKind::MangaDetails,
        metrics,
        started_at,
    );
    let cache_key = CacheKey::Details {
        plugin: name.clone(),
        id: id.clone(),
    };
    let cache_span = catalog_read.cache_lookup_span();
    let cache_started_at = Instant::now();
    let cached = async { cache.get_typed::<MangaResponse>(&cache_key).await }
        .instrument(cache_span.clone())
        .await;
    trace::record_duration(&cache_span, cache_started_at.elapsed());
    if let Some(cached) = cached {
        catalog_read.record_cache_hit(&cache_span, 1);
        return Ok(cached);
    }
    catalog_read.record_cache_miss(&cache_span);

    let pm = source_registry.read().await;
    let source_base_url = match pm.source_base_url(&name) {
        Ok(source_base_url) => source_base_url,
        Err(error) => {
            catalog_read.record_error(&error);
            return Err(AppError::from(error));
        }
    };
    let plugin_span = catalog_read.plugin_span();
    let plugin_started_at = Instant::now();
    let manga = pm
        .get_manga_details(&name, &id)
        .instrument(plugin_span.clone())
        .await;
    trace::record_duration(&plugin_span, plugin_started_at.elapsed());
    let manga = match manga {
        Ok(manga) => manga,
        Err(error) => {
            catalog_read.record_plugin_error(&plugin_span, &error);
            return Err(AppError::from(error));
        }
    };
    catalog_read.record_plugin_success(&plugin_span, 1);
    let response = match map_manga_response(manga, &source_base_url, &name) {
        Ok(response) => response,
        Err(error) => {
            catalog_read.record_error(&error);
            return Err(error);
        }
    };

    let cache_write_span = catalog_read.cache_write_span();
    let cache_write_started_at = Instant::now();
    {
        let _entered = cache_write_span.enter();
        cache.insert_typed(cache_key, &response);
    }
    trace::record_duration(&cache_write_span, cache_write_started_at.elapsed());
    catalog_read.record_success_miss(1);
    Ok(response)
}

#[autometrics]
#[tracing::instrument(name = "app.source.chapter_list", skip_all, fields(source = %name, remote_id = %id, outcome = tracing::field::Empty, cache_result = tracing::field::Empty, item_count = tracing::field::Empty))]
pub async fn get_chapter_list(
    source_registry: &RwLock<SourceRegistry>,
    cache: &MangaCache,
    metrics: &Metrics,
    name: String,
    id: String,
) -> Result<ApiListResponse<ChapterResponse>, AppError> {
    let started_at = Instant::now();
    let catalog_read = SourceCatalogRead::current(
        &name,
        SourceCatalogReadKind::ChapterList,
        metrics,
        started_at,
    );
    let cache_key = CacheKey::Chapters {
        plugin: name.clone(),
        manga_id: id.clone(),
    };
    let cache_span = catalog_read.cache_lookup_span();
    let cache_started_at = Instant::now();
    let cached = async {
        cache
            .get_typed::<ApiListResponse<ChapterResponse>>(&cache_key)
            .await
    }
    .instrument(cache_span.clone())
    .await;
    trace::record_duration(&cache_span, cache_started_at.elapsed());
    if let Some(cached) = cached {
        catalog_read.record_cache_hit(&cache_span, cached.items.len());
        return Ok(cached);
    }
    catalog_read.record_cache_miss(&cache_span);

    let pm = source_registry.read().await;
    let plugin_span = catalog_read.plugin_span();
    let plugin_started_at = Instant::now();
    let chapters = pm
        .get_chapter_list(&name, &id)
        .instrument(plugin_span.clone())
        .await;
    trace::record_duration(&plugin_span, plugin_started_at.elapsed());
    let chapters = match chapters {
        Ok(chapters) => chapters,
        Err(error) => {
            catalog_read.record_plugin_error(&plugin_span, &error);
            return Err(AppError::from(error));
        }
    };
    catalog_read.record_plugin_success(&plugin_span, chapters.len());
    let response = ApiListResponse::new(
        chapters
            .into_iter()
            .map(|chapter| ChapterResponse {
                id: chapter.id,
                title: chapter.title,
                chapter_number: chapter.number,
                date_uploaded: chapter.published_at,
            })
            .collect(),
    );

    let cache_write_span = catalog_read.cache_write_span();
    let cache_write_started_at = Instant::now();
    {
        let _entered = cache_write_span.enter();
        cache.insert_typed(cache_key, &response);
    }
    trace::record_duration(&cache_write_span, cache_write_started_at.elapsed());
    catalog_read.record_success_miss(response.items.len());
    Ok(response)
}

fn map_manga_response(
    manga: types::Manga,
    source_base_url: &str,
    source_name: &str,
) -> Result<MangaResponse, AppError> {
    let cover_url = manga.cover.url.clone();
    let (cover_proxy_url, cover_fetch_spec) = if manga.cover.request.is_some() {
        let spec = media_ref_to_spec(&manga.cover)?;
        let encoded = encode_media_spec(&spec)?;
        let cover_proxy_url = Some(crate::app::media::media_proxy_url(source_name, &encoded));
        (cover_proxy_url, Some(encoded))
    } else {
        (None, None)
    };

    Ok(MangaResponse {
        id: manga.id,
        title: manga.title,
        cover_url,
        cover_proxy_url,
        cover_fetch_spec,
        description: manga.description,
        author: manga.author,
        genres: manga.genres,
        status: manga.status,
        source_base_url: Some(source_base_url.to_owned()),
        is_nsfw: manga.is_nsfw,
    })
}
