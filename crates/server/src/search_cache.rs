use std::{sync::Arc, time::Instant};

use autometrics::autometrics;
use backend_sources::SourceInfo;
use backend_telemetry::trace;
use tokio_graceful::ShutdownGuard;
use tracing::Instrument as _;

use crate::{
    AppState,
    app::catalog::{self, SearchSourceInput},
};
use backend_cache::CacheKey;

const SEARCH_PRECACHE_INTERVAL: tokio::time::Duration = tokio::time::Duration::from_hours(1);
const SEARCH_PRECACHE_PAGE: u32 = 1;

#[derive(Debug)]
struct WarmQuery {
    source: String,
    category: String,
}

#[derive(Debug, Default)]
struct WarmSummary {
    sources: usize,
    queries: usize,
    failed_queries: usize,
}

pub async fn run_search_cache_refresher(state: Arc<AppState>, shutdown: ShutdownGuard) {
    tracing::info!(
        interval_minutes = SEARCH_PRECACHE_INTERVAL.as_secs() / 60,
        "Search Cache Refresher Started",
    );

    let shutdown = shutdown.clone_weak();
    warm_search_cache(&state).await;

    loop {
        let sleep = tokio::time::sleep(SEARCH_PRECACHE_INTERVAL);
        tokio::pin!(sleep);

        tokio::select! {
            () = shutdown.cancelled() => {
                tracing::info!("Search Cache Refresher Stopping");
                break;
            }
            () = &mut sleep => {}
        }

        warm_search_cache(&state).await;
    }

    tracing::info!("Search Cache Refresher Stopped");
}

#[autometrics(track_concurrency)]
#[tracing::instrument(name = "background.search_cache.warm", skip_all, fields(source_count = tracing::field::Empty, query_count = tracing::field::Empty, failed_queries = tracing::field::Empty, outcome = tracing::field::Empty, duration_ms = tracing::field::Empty))]
async fn warm_search_cache(state: &AppState) {
    let started_at = Instant::now();
    let span = tracing::Span::current();
    let job_span = trace::background_job_span("search_cache_warm");
    let queries = warm_queries(state).await;
    let mut summary = WarmSummary {
        sources: count_distinct_sources(&queries),
        queries: queries.len(),
        failed_queries: 0,
    };
    span.record(
        "source_count",
        u64::try_from(summary.sources).unwrap_or(u64::MAX),
    );
    span.record(
        "query_count",
        u64::try_from(summary.queries).unwrap_or(u64::MAX),
    );

    for query in queries {
        match warm_query(state, &query).instrument(job_span.clone()).await {
            Ok(result_count) => {
                state
                    .telemetry
                    .metrics
                    .record_search_cache_warm_query(&query.source, "success");
                tracing::trace!(
                    source = %query.source,
                    category = %query.category,
                    result_count,
                    "Search Cache Query Warmed",
                );
            }
            Err(error) => {
                summary.failed_queries += 1;
                state
                    .telemetry
                    .metrics
                    .record_search_cache_warm_query(&query.source, "error");
                tracing::warn!(
                    source = %query.source,
                    category = %query.category,
                    error = %error,
                    outcome = "error",
                    "Search Cache Query Warm Failed",
                );
            }
        }
    }

    state.telemetry.metrics.record_search_cache_warm_run(
        search_cache_warm_outcome(summary.queries, summary.failed_queries),
        started_at.elapsed(),
    );
    let outcome = search_cache_warm_outcome(summary.queries, summary.failed_queries);
    span.record(
        "failed_queries",
        u64::try_from(summary.failed_queries).unwrap_or(u64::MAX),
    );
    span.record(
        "duration_ms",
        u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
    );
    trace::record_outcome(&span, outcome);
    trace::record_outcome(&job_span, outcome);
    trace::record_item_count(&job_span, summary.queries);
    trace::record_duration(&job_span, started_at.elapsed());
    tracing::info!(
        sources = summary.sources,
        queries = summary.queries,
        failed_queries = summary.failed_queries,
        duration_ms = started_at.elapsed().as_millis(),
        "Search Cache Warm Completed",
    );
}

fn search_cache_warm_outcome(queries: usize, failed_queries: usize) -> &'static str {
    match (queries, failed_queries) {
        (0, _) => "empty",
        (_, 0) => "success",
        (queries, failed) if queries == failed => "error",
        _ => "partial_error",
    }
}

#[autometrics]
#[tracing::instrument(name = "background.search_cache.plan", skip_all, fields(source_count = tracing::field::Empty, query_count = tracing::field::Empty))]
async fn warm_queries(state: &AppState) -> Vec<WarmQuery> {
    let pm = state.source_registry.read().await;
    let queries = pm
        .sources()
        .into_iter()
        .filter(is_search_precache_source)
        .flat_map(|source| {
            let source_name = source.name;
            source
                .search_categories
                .into_iter()
                .filter_map(move |category| {
                    let category = category.trim();
                    (!category.is_empty()).then(|| WarmQuery {
                        source: source_name.clone(),
                        category: category.to_owned(),
                    })
                })
        })
        .collect::<Vec<_>>();
    let span = tracing::Span::current();
    span.record(
        "source_count",
        u64::try_from(count_distinct_sources(&queries)).unwrap_or(u64::MAX),
    );
    span.record(
        "query_count",
        u64::try_from(queries.len()).unwrap_or(u64::MAX),
    );
    queries
}

fn is_search_precache_source(source: &SourceInfo) -> bool {
    source.enabled
        && source
            .capabilities
            .iter()
            .any(|capability| capability == "search")
        && !source.search_categories.is_empty()
}

#[autometrics(track_concurrency)]
#[tracing::instrument(name = "background.search_cache.query", skip_all, fields(source = %query.source, category = %query.category, outcome = tracing::field::Empty, item_count = tracing::field::Empty))]
async fn warm_query(
    state: &AppState,
    query: &WarmQuery,
) -> Result<usize, crate::api::error::AppError> {
    invalidate_search_query(state, query);

    let response = catalog::search_source(
        &state.db,
        &state.source_registry,
        &state.cache,
        &state.telemetry.metrics,
        SearchSourceInput {
            name: query.source.clone(),
            query: String::new(),
            page: SEARCH_PRECACHE_PAGE,
            category: Some(query.category.clone()),
            popular: false,
        },
    )
    .await?;

    let result_count = response.mangas.len();
    let span = tracing::Span::current();
    trace::record_outcome(&span, "success");
    trace::record_item_count(&span, result_count);
    Ok(result_count)
}

#[tracing::instrument(name = "cache.invalidate", skip_all, fields(cache = "manga_response", key_kind = "source_search", source = %query.source, category = %query.category))]
fn invalidate_search_query(state: &AppState, query: &WarmQuery) {
    for hide_nsfw in [false, true] {
        state.cache.remove(&CacheKey::Search {
            plugin: query.source.clone(),
            query: String::new(),
            page: SEARCH_PRECACHE_PAGE,
            category: Some(query.category.clone()),
            popular: false,
            hide_nsfw,
        });
    }
}

fn count_distinct_sources(queries: &[WarmQuery]) -> usize {
    let mut sources = queries
        .iter()
        .map(|query| query.source.as_str())
        .collect::<Vec<_>>();
    sources.sort_unstable();
    sources.dedup();
    sources.len()
}
