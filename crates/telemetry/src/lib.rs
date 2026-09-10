#![allow(clippy::missing_errors_doc)]

use std::{env, fmt, sync::OnceLock, time::Duration};

use anyhow::Context;
use autometrics::settings::AutometricsSettings;
use metrics::{
    Label, counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram,
};
use metrics_exporter_otel::OpenTelemetryRecorder;
use opentelemetry::{KeyValue, global, metrics::MeterProvider as _, trace::TracerProvider as _};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter};
use opentelemetry_sdk::{
    Resource, logs::SdkLoggerProvider, metrics::SdkMeterProvider, trace::SdkTracerProvider,
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

const SERVICE_NAME: &str = "manga-server";

const DOWNLOADS_ENQUEUED: &str = "manga_server_downloads_enqueued_total";
const DOWNLOADS_ACTIVE: &str = "manga_server_downloads_active";
const DOWNLOADS_COMPLETED: &str = "manga_server_downloads_completed_total";
const DOWNLOAD_DURATION_SECONDS: &str = "manga_server_download_duration_seconds";
const DOWNLOAD_PAGES: &str = "manga_server_download_pages_total";
const DOWNLOAD_PAGE_FETCHES: &str = "manga_server_download_page_fetches_total";
const DOWNLOAD_PAGE_FETCH_DURATION_SECONDS: &str =
    "manga_server_download_page_fetch_duration_seconds";
const DOWNLOAD_PAGE_BYTES: &str = "manga_server_download_page_bytes_total";
const DOWNLOAD_PAGE_FETCH_CACHE: &str = "manga_server_download_page_fetch_cache_total";
const DOWNLOAD_PAGE_FETCH_RETRIES: &str = "manga_server_download_page_fetch_retries_total";
const DOWNLOAD_ARCHIVE_BYTES: &str = "manga_server_download_archive_bytes";
const DOWNLOADS_BY_STATUS: &str = "manga_server_downloads_by_status";
const DOWNLOADS_BY_SOURCE_STATUS: &str = "manga_server_downloads_by_source_status";
const LIBRARY_MANGA: &str = "manga_server_library_manga";
const SOURCES: &str = "manga_server_sources";
const SOURCE_OPERATIONS: &str = "manga_server_source_operations_total";
const SOURCE_OPERATION_DURATION_SECONDS: &str = "manga_server_source_operation_duration_seconds";
const SOURCE_OPERATION_ITEMS: &str = "manga_server_source_operation_items_total";
const SOURCE_PLUGIN_CHANGES: &str = "manga_server_source_plugin_changes_total";
const REQUEST_ERRORS: &str = "manga_server_request_errors_total";
const DOWNLOAD_STORAGE_BYTES: &str = "manga_server_download_storage_bytes";
const LIBRARY_UPDATE_RUNS: &str = "manga_server_library_update_runs_total";
const LIBRARY_UPDATE_DURATION_SECONDS: &str = "manga_server_library_update_duration_seconds";
const LIBRARY_UPDATE_NEW_CHAPTERS: &str = "manga_server_library_update_new_chapters_total";
const LIBRARY_UPDATE_ENQUEUED_DOWNLOADS: &str =
    "manga_server_library_update_enqueued_downloads_total";
const LIBRARY_UPDATE_MANGA_CHECKED: &str = "manga_server_library_update_manga_checked_total";
const MEDIA_PROXY_REQUESTS: &str = "manga_server_media_proxy_requests_total";
const MEDIA_PROXY_BYTES: &str = "manga_server_media_proxy_bytes_total";
const MEDIA_PROXY_STAGE_DURATION_SECONDS: &str = "manga_server_media_proxy_stage_duration_seconds";
const SEARCH_CACHE_WARM_RUNS: &str = "manga_server_search_cache_warm_runs_total";
const SEARCH_CACHE_WARM_DURATION_SECONDS: &str = "manga_server_search_cache_warm_duration_seconds";
const SEARCH_CACHE_WARM_QUERIES: &str = "manga_server_search_cache_warm_queries_total";
const DOWNLOADED_PAGE_REQUESTS: &str = "manga_server_downloaded_page_requests_total";
pub mod trace {
    use super::{Duration, fmt};
    use opentelemetry::trace::TraceContextExt as _;
    use tracing::{Span, field};
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;

    #[derive(Debug, Clone, Default)]
    pub struct TraceContext {
        trace_id: String,
        span_id: String,
    }

    impl TraceContext {
        #[must_use]
        pub fn trace_id(&self) -> &str {
            &self.trace_id
        }

        #[must_use]
        pub fn span_id(&self) -> &str {
            &self.span_id
        }

        #[must_use]
        pub fn has_trace(&self) -> bool {
            !self.trace_id.is_empty()
        }
    }

    pub fn cache_lookup_span(cache: &'static str, key_kind: &'static str) -> Span {
        tracing::info_span!(
            "cache.lookup",
            cache,
            key_kind,
            result = field::Empty,
            duration_ms = field::Empty,
        )
    }

    pub fn cache_write_span(cache: &'static str, key_kind: &'static str) -> Span {
        tracing::info_span!("cache.write", cache, key_kind, duration_ms = field::Empty,)
    }

    pub fn db_operation_span(operation: &'static str, entity: &'static str) -> Span {
        tracing::info_span!(
            "db.operation",
            operation,
            entity,
            outcome = field::Empty,
            rows = field::Empty,
            duration_ms = field::Empty,
            error = field::Empty,
        )
    }

    pub fn plugin_operation_span(source: &str, operation: &'static str) -> Span {
        tracing::info_span!(
            "plugin.operation",
            source = %source,
            operation,
            outcome = field::Empty,
            item_count = field::Empty,
            duration_ms = field::Empty,
            error = field::Empty,
        )
    }

    pub fn background_job_span(job: &'static str) -> Span {
        tracing::info_span!(
            "background.job",
            job,
            outcome = field::Empty,
            item_count = field::Empty,
            duration_ms = field::Empty,
            error = field::Empty,
        )
    }

    #[must_use]
    pub fn current_trace_context() -> TraceContext {
        let context = Span::current().context();
        let span = context.span();
        let span_context = span.span_context();
        if !span_context.is_valid() {
            return TraceContext::default();
        }

        TraceContext {
            trace_id: span_context.trace_id().to_string(),
            span_id: span_context.span_id().to_string(),
        }
    }

    pub fn record_current_trace_context(span: &Span) {
        let context = current_trace_context();
        if !context.has_trace() {
            return;
        }
        span.record("trace_id", context.trace_id());
        span.record("span_id", context.span_id());
    }

    pub fn record_result(span: &Span, result: &'static str) {
        span.record("result", result);
        span.record("cache_result", result);
    }

    pub fn record_outcome(span: &Span, outcome: &'static str) {
        span.record("outcome", outcome);
    }

    pub fn record_rows(span: &Span, rows: usize) {
        span.record("rows", u64::try_from(rows).unwrap_or(u64::MAX));
    }

    pub fn record_item_count(span: &Span, item_count: usize) {
        span.record("item_count", u64::try_from(item_count).unwrap_or(u64::MAX));
    }

    pub fn record_duration(span: &Span, duration: Duration) {
        span.record(
            "duration_ms",
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        );
    }

    pub fn record_error<E: fmt::Display + ?Sized>(span: &Span, error: &E) {
        span.record("outcome", "error");
        span.record("error", field::display(error));
    }
}

#[derive(Clone)]
pub struct Telemetry {
    pub metrics: Metrics,
    otel: Option<OtelTelemetry>,
}

#[derive(Clone)]
struct OtelTelemetry {
    tracer: SdkTracerProvider,
    meter: SdkMeterProvider,
    logger: SdkLoggerProvider,
}

impl Telemetry {
    /// Initializes tracing, OTLP exporters, metrics, and Autometrics settings.
    ///
    /// # Errors
    ///
    /// Returns an error if OpenTelemetry or Autometrics cannot be initialized.
    pub fn init(default_filter: &str, color_logs: bool) -> anyhow::Result<Self> {
        let resource = otel_resource();
        let otel = if otel_disabled() {
            None
        } else {
            Some(init_otel(resource.clone())?)
        };

        init_metrics_recorder(otel.as_ref())?;
        init_tracing(default_filter, color_logs, otel.as_ref())?;

        AutometricsSettings::builder()
            .service_name(SERVICE_NAME)
            .try_init()?;

        Ok(Self {
            metrics: Metrics::new(),
            otel,
        })
    }

    #[doc(hidden)]
    /// Returns a singleton no-op telemetry handle for tests.
    pub fn for_test(_prefix: &'static str) -> Self {
        static TEST_TELEMETRY: OnceLock<Telemetry> = OnceLock::new();
        TEST_TELEMETRY
            .get_or_init(|| Self {
                metrics: Metrics::default(),
                otel: None,
            })
            .clone()
    }

    /// Flushes and shuts down OpenTelemetry providers owned by this handle.
    pub fn shutdown(&self) -> anyhow::Result<()> {
        if let Some(otel) = &self.otel {
            otel.logger
                .shutdown()
                .context("failed to shut down OpenTelemetry log provider")?;
            otel.meter
                .shutdown()
                .context("failed to shut down OpenTelemetry meter provider")?;
            otel.tracer
                .shutdown()
                .context("failed to shut down OpenTelemetry tracer provider")?;
        }
        Ok(())
    }
}

fn otel_resource() -> Resource {
    let mut attrs = Vec::new();
    push_default_resource_attr(
        &mut attrs,
        "service.namespace",
        "OTEL_SERVICE_NAMESPACE",
        "manga",
    );
    push_default_resource_attr(
        &mut attrs,
        "deployment.environment",
        "MANGA_DEPLOYMENT_ENVIRONMENT",
        "local",
    );
    push_default_resource_attr(&mut attrs, "service.version", "MANGA_SERVER_VERSION", "dev");
    if !resource_attr_configured("service.instance.id") {
        attrs.push(KeyValue::new(
            "service.instance.id",
            env_value("HOSTNAME")
                .or_else(|| env_value("HOST"))
                .unwrap_or_else(|| "local".to_owned()),
        ));
    }

    Resource::builder()
        .with_service_name(service_name())
        .with_attributes(attrs)
        .build()
}

fn push_default_resource_attr(
    attrs: &mut Vec<KeyValue>,
    key: &'static str,
    env_key: &str,
    default: &'static str,
) {
    if resource_attr_configured(key) {
        return;
    }
    attrs.push(KeyValue::new(
        key,
        env_value(env_key).unwrap_or_else(|| default.to_owned()),
    ));
}

fn service_name() -> String {
    env_value("OTEL_SERVICE_NAME").unwrap_or_else(|| SERVICE_NAME.to_owned())
}

fn env_value(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn resource_attr_configured(key: &str) -> bool {
    let Some(attrs) = env_value("OTEL_RESOURCE_ATTRIBUTES") else {
        return false;
    };
    attrs
        .split(',')
        .filter_map(|entry| entry.split_once('='))
        .any(|(entry_key, _)| entry_key.trim() == key)
}

fn init_otel(resource: Resource) -> anyhow::Result<OtelTelemetry> {
    let span_exporter = SpanExporter::builder()
        .with_http()
        .build()
        .context("failed to build OTLP trace exporter")?;
    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(span_exporter)
        .build();
    global::set_tracer_provider(tracer_provider.clone());

    let metric_exporter = MetricExporter::builder()
        .with_http()
        .build()
        .context("failed to build OTLP metric exporter")?;
    let meter_provider = SdkMeterProvider::builder()
        .with_resource(resource.clone())
        .with_periodic_exporter(metric_exporter)
        .build();
    global::set_meter_provider(meter_provider.clone());

    let log_exporter = LogExporter::builder()
        .with_http()
        .build()
        .context("failed to build OTLP log exporter")?;
    let logger_provider = SdkLoggerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(log_exporter)
        .build();

    Ok(OtelTelemetry {
        tracer: tracer_provider,
        meter: meter_provider,
        logger: logger_provider,
    })
}

fn init_metrics_recorder(otel: Option<&OtelTelemetry>) -> anyhow::Result<()> {
    if let Some(otel) = otel {
        let meter = otel.meter.meter(SERVICE_NAME);
        let recorder = OpenTelemetryRecorder::new(meter);
        metrics::set_global_recorder(recorder)
            .context("failed to install OpenTelemetry metrics recorder")?;
    }
    Ok(())
}

fn init_tracing(
    default_filter: &str,
    color_logs: bool,
    otel: Option<&OtelTelemetry>,
) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| default_filter.into());
    let filter = filter
        .add_directive("opentelemetry=off".parse()?)
        .add_directive("hyper=off".parse()?)
        .add_directive("tonic=off".parse()?)
        .add_directive("h2=off".parse()?)
        .add_directive("reqwest=off".parse()?);
    let fmt_layer = tracing_subscriber::fmt::layer().with_ansi(color_logs);
    let trace_layer = otel.map(|otel| {
        let tracer = otel.tracer.tracer(SERVICE_NAME);
        tracing_opentelemetry::layer().with_tracer(tracer)
    });
    let log_layer = otel.map(|otel| OpenTelemetryTracingBridge::new(&otel.logger));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(trace_layer)
        .with(log_layer)
        .try_init()
        .context("failed to initialize tracing subscriber")
}

fn otel_disabled() -> bool {
    env::var("OTEL_SDK_DISABLED")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

pub fn record_request_error(status_code: u16, code: &str, outcome: &str) {
    counter!(
        REQUEST_ERRORS,
        vec![
            Label::new("status_code", status_code.to_string()),
            Label::new("code", code.to_owned()),
            Label::new("outcome", outcome.to_owned()),
        ]
    )
    .increment(1);
}

/// Runs the Tokio runtime metrics reporter with manga-server metric names.
pub async fn run_tokio_runtime_metrics_reporter() {
    tokio_metrics::RuntimeMetricsReporterBuilder::default()
        .with_metrics_transformer(|name| {
            metrics::Key::from_name(name.replacen("tokio_", "manga_server_tokio_", 1))
        })
        .describe_and_run()
        .await;
}

#[derive(Clone, Copy, Default)]
pub struct Metrics {
    recorder: (),
}

impl Metrics {
    #[allow(clippy::too_many_lines)]
    fn new() -> Self {
        describe_counter!(DOWNLOADS_ENQUEUED, "Downloads accepted into the queue.");
        describe_gauge!(DOWNLOADS_ACTIVE, "Downloads currently being processed.");
        describe_counter!(DOWNLOADS_COMPLETED, "Downloads completed by outcome.");
        describe_histogram!(
            DOWNLOAD_DURATION_SECONDS,
            "End-to-end chapter download duration in seconds."
        );
        describe_counter!(DOWNLOAD_PAGES, "Pages included in completed downloads.");
        describe_counter!(
            DOWNLOAD_PAGE_FETCHES,
            "Individual page fetches by source and outcome."
        );
        describe_histogram!(
            DOWNLOAD_PAGE_FETCH_DURATION_SECONDS,
            "Individual page fetch duration in seconds."
        );
        describe_counter!(DOWNLOAD_PAGE_BYTES, "Bytes fetched for downloaded pages.");
        describe_counter!(
            DOWNLOAD_PAGE_FETCH_CACHE,
            "Page fetch cache lookups by source and result."
        );
        describe_counter!(DOWNLOAD_PAGE_FETCH_RETRIES, "Page fetch retries by source.");
        describe_histogram!(
            DOWNLOAD_ARCHIVE_BYTES,
            "Archive size for completed downloads in bytes."
        );
        describe_gauge!(
            DOWNLOADS_BY_STATUS,
            "Downloads currently recorded by status."
        );
        describe_gauge!(
            DOWNLOADS_BY_SOURCE_STATUS,
            "Downloads currently recorded by source and status."
        );
        describe_gauge!(LIBRARY_MANGA, "Library manga currently recorded by source.");
        describe_gauge!(SOURCES, "Configured source plugins by state.");
        describe_counter!(
            SOURCE_OPERATIONS,
            "Source plugin operations by source, operation, outcome, and cache result."
        );
        describe_histogram!(
            SOURCE_OPERATION_DURATION_SECONDS,
            "Source plugin operation duration in seconds by source, operation, outcome, and cache result."
        );
        describe_counter!(
            SOURCE_OPERATION_ITEMS,
            "Items returned by successful source plugin operations by source and operation."
        );
        describe_counter!(
            SOURCE_PLUGIN_CHANGES,
            "Source plugin lifecycle changes by source, operation, and outcome."
        );
        describe_counter!(
            REQUEST_ERRORS,
            "API errors returned to clients by status code, error code, and outcome."
        );
        describe_gauge!(DOWNLOAD_STORAGE_BYTES, "Download storage bytes by kind.");
        describe_counter!(
            LIBRARY_UPDATE_RUNS,
            "Library update runs by trigger and outcome."
        );
        describe_histogram!(
            LIBRARY_UPDATE_DURATION_SECONDS,
            "Library update run duration in seconds."
        );
        describe_counter!(
            LIBRARY_UPDATE_NEW_CHAPTERS,
            "New chapters discovered by library update runs."
        );
        describe_counter!(
            LIBRARY_UPDATE_ENQUEUED_DOWNLOADS,
            "Downloads enqueued by library update runs."
        );
        describe_counter!(
            LIBRARY_UPDATE_MANGA_CHECKED,
            "Manga checked by library update runs."
        );
        describe_counter!(
            MEDIA_PROXY_REQUESTS,
            "Media proxy requests by source, format, and outcome."
        );
        describe_counter!(
            MEDIA_PROXY_BYTES,
            "Media proxy response bytes by source and format."
        );
        describe_histogram!(
            MEDIA_PROXY_STAGE_DURATION_SECONDS,
            "Media proxy stage duration in seconds by source, format, stage, and outcome."
        );
        describe_counter!(SEARCH_CACHE_WARM_RUNS, "Search cache warm runs by outcome.");
        describe_histogram!(
            SEARCH_CACHE_WARM_DURATION_SECONDS,
            "Search cache warm run duration in seconds."
        );
        describe_counter!(
            SEARCH_CACHE_WARM_QUERIES,
            "Search cache warm queries by source and outcome."
        );
        describe_counter!(
            DOWNLOADED_PAGE_REQUESTS,
            "Downloaded reader page requests by page-index bucket and outcome."
        );
        Self { recorder: () }
    }

    /// Records that a download was accepted into the queue.
    pub fn record_download_enqueued(&self, source: &str, trigger: &str) {
        counter!(
            DOWNLOADS_ENQUEUED,
            self.labels([("source", source), ("trigger", trigger)])
        )
        .increment(1);
    }

    /// Increments the active-download gauge for a source.
    pub fn download_started(&self, source: &str) {
        gauge!(DOWNLOADS_ACTIVE, self.labels([("source", source)])).increment(1.0);
    }

    /// Decrements the active-download gauge for a source.
    pub fn download_finished(&self, source: &str) {
        gauge!(DOWNLOADS_ACTIVE, self.labels([("source", source)])).decrement(1.0);
    }

    /// Records the outcome, duration, page count, and optional archive size for a download.
    pub fn record_download_completed(
        &self,
        source: &str,
        outcome: &str,
        duration: Duration,
        pages: usize,
        archive_bytes: Option<u64>,
    ) {
        counter!(
            DOWNLOADS_COMPLETED,
            self.labels([("source", source), ("outcome", outcome)])
        )
        .increment(1);
        histogram!(
            DOWNLOAD_DURATION_SECONDS,
            self.labels([("source", source), ("outcome", outcome)])
        )
        .record(duration);

        if outcome == "success" {
            counter!(DOWNLOAD_PAGES, self.labels([("source", source)]))
                .increment(u64::try_from(pages).unwrap_or(u64::MAX));

            if let Some(bytes) = archive_bytes {
                histogram!(DOWNLOAD_ARCHIVE_BYTES, self.labels([("source", source)]))
                    .record(u64_to_f64(bytes));
            }
        }
    }

    /// Sets the current download count for one public status label.
    pub fn set_downloads_by_status(&self, status: &str, count: u64) {
        gauge!(DOWNLOADS_BY_STATUS, self.labels([("status", status)])).set(u64_to_f64(count));
    }

    /// Sets the current download count for a source/status pair.
    pub fn set_downloads_by_source_status(&self, source: &str, status: &str, count: u64) {
        gauge!(
            DOWNLOADS_BY_SOURCE_STATUS,
            self.labels([("source", source), ("status", status)])
        )
        .set(u64_to_f64(count));
    }

    /// Sets the current library manga count for a source.
    pub fn set_library_manga(&self, source: &str, count: u64) {
        gauge!(LIBRARY_MANGA, self.labels([("source", source)])).set(u64_to_f64(count));
    }

    /// Sets the current source count for a source state.
    pub fn set_sources(&self, state: &str, count: u64) {
        gauge!(SOURCES, self.labels([("state", state)])).set(u64_to_f64(count));
    }

    /// Records one source-plugin read operation.
    pub fn record_source_operation(
        &self,
        source: &str,
        operation: &'static str,
        outcome: &str,
        cache_result: &'static str,
        duration: Duration,
        item_count: Option<usize>,
    ) {
        counter!(
            SOURCE_OPERATIONS,
            self.labels([
                ("source", source),
                ("operation", operation),
                ("outcome", outcome),
                ("cache_result", cache_result),
            ])
        )
        .increment(1);
        histogram!(
            SOURCE_OPERATION_DURATION_SECONDS,
            self.labels([
                ("source", source),
                ("operation", operation),
                ("outcome", outcome),
                ("cache_result", cache_result),
            ])
        )
        .record(duration);
        if let Some(count) = item_count {
            counter!(
                SOURCE_OPERATION_ITEMS,
                self.labels([("source", source), ("operation", operation)])
            )
            .increment(u64::try_from(count).unwrap_or(u64::MAX));
        }
    }

    /// Records a source-plugin lifecycle mutation.
    pub fn record_source_plugin_change(
        &self,
        source: &str,
        operation: &'static str,
        outcome: &str,
    ) {
        counter!(
            SOURCE_PLUGIN_CHANGES,
            self.labels([
                ("source", source),
                ("operation", operation),
                ("outcome", outcome),
            ])
        )
        .increment(1);
    }

    /// Sets a download-storage byte gauge by storage kind.
    pub fn set_download_storage_bytes(&self, kind: &str, bytes: u64) {
        gauge!(DOWNLOAD_STORAGE_BYTES, self.labels([("kind", kind)])).set(u64_to_f64(bytes));
    }

    /// Records a successful page fetch with latency and byte count.
    pub fn record_page_fetch_success(&self, source: &str, duration_ms: u64, bytes: u64) {
        counter!(
            DOWNLOAD_PAGE_FETCHES,
            self.labels([("source", source), ("outcome", "success")])
        )
        .increment(1);
        histogram!(
            DOWNLOAD_PAGE_FETCH_DURATION_SECONDS,
            self.labels([("source", source)])
        )
        .record(Duration::from_millis(duration_ms));
        counter!(DOWNLOAD_PAGE_BYTES, self.labels([("source", source)])).increment(bytes);
    }

    /// Records a failed page fetch for a source.
    pub fn record_page_fetch_error(&self, source: &str) {
        counter!(
            DOWNLOAD_PAGE_FETCHES,
            self.labels([("source", source), ("outcome", "error")])
        )
        .increment(1);
    }

    /// Records a page-fetch cache hit/miss-style result.
    pub fn record_page_fetch_cache(&self, source: &str, result: &str) {
        counter!(
            DOWNLOAD_PAGE_FETCH_CACHE,
            self.labels([("source", source), ("result", result)])
        )
        .increment(1);
    }

    /// Records a page-fetch retry for a source.
    pub fn record_page_fetch_retry(&self, source: &str) {
        counter!(
            DOWNLOAD_PAGE_FETCH_RETRIES,
            self.labels([("source", source)])
        )
        .increment(1);
    }

    /// Records one library update run and its aggregate effects.
    pub fn record_library_update_run(
        &self,
        trigger: &str,
        outcome: &str,
        duration: Duration,
        new_chapters: usize,
        enqueued_downloads: usize,
    ) {
        counter!(
            LIBRARY_UPDATE_RUNS,
            self.labels([("trigger", trigger), ("outcome", outcome)])
        )
        .increment(1);
        histogram!(
            LIBRARY_UPDATE_DURATION_SECONDS,
            self.labels([("trigger", trigger), ("outcome", outcome)])
        )
        .record(duration);
        counter!(
            LIBRARY_UPDATE_NEW_CHAPTERS,
            self.labels([("trigger", trigger)])
        )
        .increment(u64::try_from(new_chapters).unwrap_or(u64::MAX));
        counter!(
            LIBRARY_UPDATE_ENQUEUED_DOWNLOADS,
            self.labels([("trigger", trigger)])
        )
        .increment(u64::try_from(enqueued_downloads).unwrap_or(u64::MAX));
    }

    /// Records the outcome of checking one manga during a library update.
    pub fn record_library_update_manga_checked(&self, trigger: &str, source: &str, outcome: &str) {
        counter!(
            LIBRARY_UPDATE_MANGA_CHECKED,
            self.labels([
                ("trigger", trigger),
                ("source", source),
                ("outcome", outcome),
            ])
        )
        .increment(1);
    }

    /// Records a media proxy request and optional response byte count.
    pub fn record_media_proxy_request(
        &self,
        source: &str,
        format: &str,
        outcome: &str,
        bytes: Option<u64>,
    ) {
        counter!(
            MEDIA_PROXY_REQUESTS,
            self.labels([("source", source), ("format", format), ("outcome", outcome),])
        )
        .increment(1);
        if let Some(bytes) = bytes {
            counter!(
                MEDIA_PROXY_BYTES,
                self.labels([("source", source), ("format", format)])
            )
            .increment(bytes);
        }
    }

    /// Records timing for one media proxy pipeline stage.
    pub fn record_media_proxy_stage(
        &self,
        source: &str,
        format: &str,
        stage: &'static str,
        outcome: &str,
        duration: Duration,
    ) {
        histogram!(
            MEDIA_PROXY_STAGE_DURATION_SECONDS,
            self.labels([
                ("source", source),
                ("format", format),
                ("stage", stage),
                ("outcome", outcome),
            ])
        )
        .record(duration);
    }

    /// Records a search-cache warm run outcome and duration.
    pub fn record_search_cache_warm_run(&self, outcome: &str, duration: Duration) {
        counter!(SEARCH_CACHE_WARM_RUNS, self.labels([("outcome", outcome)])).increment(1);
        histogram!(
            SEARCH_CACHE_WARM_DURATION_SECONDS,
            self.labels([("outcome", outcome)])
        )
        .record(duration);
    }

    /// Records one source query during search-cache warming.
    pub fn record_search_cache_warm_query(&self, source: &str, outcome: &str) {
        counter!(
            SEARCH_CACHE_WARM_QUERIES,
            self.labels([("source", source), ("outcome", outcome)])
        )
        .increment(1);
    }

    /// Records a downloaded-reader page request outcome.
    pub fn record_downloaded_page_request(&self, page_bucket: &'static str, outcome: &str) {
        counter!(
            DOWNLOADED_PAGE_REQUESTS,
            self.labels([("page_bucket", page_bucket), ("outcome", outcome)])
        )
        .increment(1);
    }

    fn labels<const N: usize>(self, pairs: [(&'static str, &str); N]) -> Vec<Label> {
        let () = self.recorder;
        labels(pairs)
    }
}

fn labels<const N: usize>(pairs: [(&'static str, &str); N]) -> Vec<Label> {
    pairs
        .into_iter()
        .map(|(key, value)| Label::new(key, value.to_owned()))
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn u64_to_f64(value: u64) -> f64 {
    value as f64
}
