import * as dashboard from "@grafana/grafana-foundation-sdk/dashboard";

import {
  alertListPanel,
  base,
  barChartPanel,
  gaugePanel,
  piePanel,
  prom,
  queryVariable,
  row,
  statPanel,
  tablePanel,
  textPanel,
  timeseriesPanel,
} from "./dashboard-helpers.js";
import { mangaServerOverviewSignals } from "./telemetry-signals.js";

export function mangaServerOverview() {
  const signals = mangaServerOverviewSignals();
  const serviceMatcher = 'service_name="$job"';
  const httpRequestCount = `{__name__="http.server.request.duration_count", ${serviceMatcher}}`;
  const httpRequestCountByRoute = `{__name__="http.server.request.duration_count", ${serviceMatcher}, "http.route"=~"$route"}`;
  const httpRequest5xx = `{__name__="http.server.request.duration_count", ${serviceMatcher}, "http.response.status_code"=~"5.."}`;
  const httpRequestBuckets = `{__name__="http.server.request.duration_bucket", ${serviceMatcher}}`;
  const httpRequestBucketsByRoute = `{__name__="http.server.request.duration_bucket", ${serviceMatcher}, "http.route"=~"$route"}`;
  const httpActiveRequests = `{__name__="http.server.active_requests", ${serviceMatcher}}`;
  const targetPresent =
    'max(build_info{service_name="$job"}) or max(manga_server_sources{service_name="$job"}) or on() vector(0)';
  const httpRequestRate = `sum(rate(${httpRequestCount}[$__rate_interval])) or on() vector(0)`;
  const httpRequestRateByRoute = `sum by (http.route, http.request.method, http.response.status_code) (rate(${httpRequestCountByRoute}[$__rate_interval]))`;
  const http5xxRatio = `(sum(rate(${httpRequest5xx}[$__rate_interval])) or on() vector(0)) / clamp_min(sum(rate(${httpRequestCount}[$__rate_interval])), 0.001)`;
  const httpLatencyP95 = `histogram_quantile(0.95, sum by (service_name, le) (rate(${httpRequestBuckets}[$__rate_interval]))) or on() vector(0)`;
  const httpLatencyP95ByRoute = `histogram_quantile(0.95, sum by (service_name, le, http.route, http.request.method) (rate(${httpRequestBucketsByRoute}[$__rate_interval])))`;
  const httpLatencyP99ByRoute = `histogram_quantile(0.99, sum by (service_name, le, http.route, http.request.method) (rate(${httpRequestBucketsByRoute}[$__rate_interval])))`;
  const foyerHitRatio =
    'sum(rate(foyer_hybrid_op_total{service_name="$job", op="hit"}[$__rate_interval])) / clamp_min(sum(rate(foyer_hybrid_op_total{service_name="$job", op=~"hit|miss"}[$__rate_interval])), 0.001)';
  const archiveIndexRequestRate = signals.downloadedArchiveIndex.requestRate;
  const archiveIndexBuildRate = signals.downloadedArchiveIndex.buildRate;
  const archiveIndexBuildAvg = signals.downloadedArchiveIndex.buildAvg;
  const storageUsedRatio = signals.downloadWorkState.storageUsedRatio;
  const readerPageP95 = signals.downloadedChapterPage.readerPageP95;
  const downloadedPageRequestRate = signals.downloadedChapterPage.requestRate;
  const downloadDurationP95 = signals.downloadWorkState.durationP95;
  const downloadPageFetchP95 = signals.downloadWorkState.pageFetchP95;
  const libraryUpdateDurationP95 =
    'manga_server_library_update_duration_seconds{service_name="$job", quantile="0.95"} or on() vector(0)';
  const downloadedPageExtractAvg = signals.downloadedChapterPage.extractAvg;
  const downloadedPageExtractPages = signals.downloadedChapterPage.extractPages;
  const downloadedPageExtractWindowPages = signals.downloadedChapterPage.extractWindowPages;
  const downloadArchiveBytesAvg = signals.downloadWorkState.archiveBytesAvg;
  const mediaProxyStageAvg =
    '(sum by (stage, outcome) (rate(manga_server_media_proxy_stage_duration_seconds_sum{service_name="$job", source=~"$source"}[$__rate_interval])) / clamp_min(sum by (stage, outcome) (rate(manga_server_media_proxy_stage_duration_seconds_count{service_name="$job", source=~"$source"}[$__rate_interval])), 0.001)) or on() vector(0)';
  const searchWarmDurationAvg =
    '(sum by (outcome) (rate(manga_server_search_cache_warm_duration_seconds_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum by (outcome) (rate(manga_server_search_cache_warm_duration_seconds_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)';
  const sourceOperationRate = signals.sourceCatalog.operationRate;
  const sourceOperationDurationAvg = signals.sourceCatalog.operationDurationAvg;
  const sourceOperationItemsRate = signals.sourceCatalog.operationItemsRate;
  const sourcePluginChanges = signals.sourceCatalog.pluginChanges;
  const requestErrorRate =
    'sum by (status_code, code, outcome) (rate(manga_server_request_errors_total{service_name="$job"}[$__rate_interval])) or on() vector(0)';
  const downloadedPageQueueWaitAvg = signals.downloadedChapterPageExtractionScheduler.queueWaitAvg;
  const downloadedPageBatchRequestsAvg =
    signals.downloadedChapterPageExtractionScheduler.batchRequestsAvg;
  const downloadedPageBatchArchivesAvg =
    signals.downloadedChapterPageExtractionScheduler.batchArchivesAvg;
  const downloadedPageGroupRequestsAvg =
    signals.downloadedChapterPageExtractionScheduler.groupRequestsAvg;
  const downloadedPageGroupPagesAvg =
    signals.downloadedChapterPageExtractionScheduler.groupPagesAvg;
  const downloadedPageWorkerAvg = signals.downloadedChapterPageExtractionScheduler.workerAvg;
  const downloadedPageResponseBuildAvg = signals.downloadedChapterPage.responseBuildAvg;

  const builder = base(
    "Manga Server Overview",
    "manga-server-overview",
    ["manga-server", "prometheus", "observability"],
    "now-3h",
    "10s",
    203,
  )
    .description(
      "Operational overview for HTTP, downloads, library updates, media work, resilience, cache, runtime pressure, and alert posture.",
    )
    .withVariable(
      queryVariable(
        "job",
        "Service",
        'label_values({__name__=~"build_info|manga_server_.*|http[.]server[.]request[.]duration_count|function_calls_total|manga_server_tokio_.*"}, service_name)',
      ),
    )
    .withVariable(
      queryVariable(
        "route",
        "Route",
        'label_values({__name__="http.server.request.duration_count", service_name="$job"}, http.route)',
        "All",
        true,
      ),
    )
    .withVariable(
      queryVariable(
        "source",
        "Source",
        'label_values({__name__=~"manga_server_downloads_by_source_status|manga_server_source_operations_total|manga_server_library_manga", service_name="$job"}, source)',
        "All",
        true,
      ),
    )
    .withVariable(
      queryVariable(
        "cache",
        "Cache",
        'label_values(foyer_hybrid_op_total{service_name="$job"}, name)',
        "All",
        true,
      ),
    );

  [
    statPanel(1, "Up", { x: 0, y: 0, w: 3, h: 4 }, targetPresent, "none", 0),
    statPanel(2, "RPS", { x: 3, y: 0, w: 3, h: 4 }, httpRequestRate, "reqps", 2),
    statPanel(3, "5xx", { x: 6, y: 0, w: 3, h: 4 }, http5xxRatio, "percentunit", 3),
    statPanel(4, "p95", { x: 9, y: 0, w: 3, h: 4 }, httpLatencyP95, "s", 3),
    statPanel(
      5,
      "Busy",
      { x: 12, y: 0, w: 3, h: 4 },
      `sum(${httpActiveRequests}) or on() vector(0)`,
      "short",
      0,
    ),
    statPanel(
      6,
      "Q",
      { x: 15, y: 0, w: 3, h: 4 },
      'sum(manga_server_downloads_by_source_status{service_name="$job", source=~"$source", status=~"queued|fetch|conversion|archive|canceling"}) or on() vector(0)',
      "short",
      0,
    ),
    gaugePanel(7, "Disk", { x: 18, y: 0, w: 3, h: 4 }, storageUsedRatio),
    gaugePanel(8, "Hit", { x: 21, y: 0, w: 3, h: 4 }, foyerHitRatio),
    row(10, "HTTP", 4),
    timeseriesPanel(
      11,
      "Request rate by route",
      { x: 0, y: 5, w: 8, h: 8 },
      [
        prom(httpRequestRate, "total requests"),
        prom(
          httpRequestRateByRoute,
          "{{http.request.method}} {{http.route}} {{http.response.status_code}}",
          "B",
        ),
      ],
      "reqps",
    ),
    timeseriesPanel(
      12,
      "Latency p95/p99 by route",
      { x: 8, y: 5, w: 8, h: 8 },
      [
        prom(httpLatencyP95ByRoute, "p95 {{http.request.method}} {{http.route}}"),
        prom(httpLatencyP99ByRoute, "p99 {{http.request.method}} {{http.route}}", "B"),
      ],
      "s",
      3,
    ),
    timeseriesPanel(
      13,
      "In-flight requests",
      { x: 16, y: 5, w: 8, h: 8 },
      [
        prom(
          `sum by (http.request.method, url.scheme) (${httpActiveRequests})`,
          "{{http.request.method}} {{url.scheme}}",
        ),
      ],
      "short",
      0,
    ),
    row(20, "Download Pipeline", 13),
    timeseriesPanel(
      21,
      "Active and queued downloads",
      { x: 0, y: 14, w: 8, h: 8 },
      [
        prom(
          'sum(manga_server_downloads_by_source_status{service_name="$job", source=~"$source", status=~"fetch|conversion|archive|canceling"})',
          "active",
        ),
        prom(
          'sum(manga_server_downloads_by_source_status{service_name="$job", source=~"$source", status="queued"})',
          "queued",
          "B",
        ),
      ],
      "short",
      0,
    ),
    timeseriesPanel(
      22,
      "Download outcomes",
      { x: 8, y: 14, w: 8, h: 8 },
      [
        prom(
          'sum by (outcome) (increase(manga_server_downloads_completed_total{service_name="$job", source=~"$source"}[$__range]))',
          "{{outcome}}",
        ),
      ],
      "short",
      0,
    ),
    timeseriesPanel(
      23,
      "Download duration",
      { x: 16, y: 14, w: 8, h: 8 },
      [prom(downloadDurationP95, "p95 {{source}} {{outcome}}")],
      "s",
      2,
    ),
    timeseriesPanel(
      24,
      "Page fetches and retries",
      { x: 0, y: 22, w: 8, h: 8 },
      [
        prom(
          'sum by (source, outcome) (rate(manga_server_download_page_fetches_total{service_name="$job", source=~"$source"}[$__rate_interval]))',
          "{{source}} {{outcome}}",
        ),
        prom(
          'sum by (source) (rate(manga_server_download_page_fetch_retries_total{service_name="$job", source=~"$source"}[$__rate_interval]))',
          "retry {{source}}",
          "B",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      25,
      "Page latency and throughput",
      { x: 8, y: 22, w: 8, h: 8 },
      [
        prom(downloadPageFetchP95, "p95 {{source}}"),
        prom(
          'sum by (source) (rate(manga_server_download_page_bytes_total{service_name="$job", source=~"$source"}[$__rate_interval])) or on() vector(0)',
          "bytes/s {{source}}",
          "B",
        ),
      ],
      "short",
    ),
    timeseriesPanel(
      26,
      "Page cache hit ratio",
      { x: 16, y: 22, w: 8, h: 8 },
      [
        prom(
          'sum by (source) (rate(manga_server_download_page_fetch_cache_total{service_name="$job", source=~"$source", result="hit"}[$__rate_interval])) / clamp_min(sum by (source) (rate(manga_server_download_page_fetch_cache_total{service_name="$job", source=~"$source"}[$__rate_interval])), 0.001)',
          "{{source}}",
        ),
      ],
      "percentunit",
      2,
    ),
    timeseriesPanel(
      27,
      "Download intake rate",
      { x: 0, y: 30, w: 8, h: 8 },
      [
        prom(
          'sum by (source, trigger) (rate(manga_server_downloads_enqueued_total{service_name="$job", source=~"$source"}[$__rate_interval]))',
          "enqueued {{source}} {{trigger}}",
        ),
        prom(
          'sum by (source) (rate(manga_server_download_pages_total{service_name="$job", source=~"$source"}[$__rate_interval]))',
          "pages {{source}}",
          "B",
        ),
      ],
      "ops",
      2,
    ),
    timeseriesPanel(
      28,
      "Download activity gauges",
      { x: 8, y: 30, w: 8, h: 8 },
      [
        prom(
          'sum by (source) (manga_server_downloads_active{service_name="$job", source=~"$source"}) or on() vector(0)',
          "active {{source}}",
        ),
        prom(
          'sum by (status) (manga_server_downloads_by_status{service_name="$job"}) or on() vector(0)',
          "status {{status}}",
          "B",
        ),
      ],
      "short",
      0,
    ),
    timeseriesPanel(
      29,
      "Archive output size",
      { x: 16, y: 30, w: 8, h: 8 },
      [prom(downloadArchiveBytesAvg, "avg {{source}}")],
      "bytes",
      0,
    ),
    row(30, "Library and Sources", 38),
    timeseriesPanel(
      31,
      "Library update outcomes",
      { x: 0, y: 39, w: 8, h: 8 },
      [
        prom(
          'sum by (outcome) (rate(manga_server_library_update_runs_total{service_name="$job"}[$__rate_interval]))',
          "{{outcome}}",
        ),
      ],
      "ops",
      2,
    ),
    timeseriesPanel(
      32,
      "New chapters, checks, and enqueues",
      { x: 8, y: 39, w: 8, h: 8 },
      [
        prom(
          'sum by (trigger) (rate(manga_server_library_update_new_chapters_total{service_name="$job"}[$__rate_interval]))',
          "new {{trigger}}",
        ),
        prom(
          'sum by (trigger) (rate(manga_server_library_update_enqueued_downloads_total{service_name="$job"}[$__rate_interval]))',
          "enqueued {{trigger}}",
          "B",
        ),
        prom(
          'sum by (source, outcome) (rate(manga_server_library_update_manga_checked_total{service_name="$job", source=~"$source"}[$__rate_interval]))',
          "checked {{source}} {{outcome}}",
          "C",
        ),
      ],
      "ops",
      2,
    ),
    timeseriesPanel(
      33,
      "Library update duration",
      { x: 16, y: 39, w: 8, h: 8 },
      [prom(libraryUpdateDurationP95, "p95 {{trigger}} {{outcome}}")],
      "s",
      2,
    ),
    barChartPanel(
      34,
      "Download queue by status",
      { x: 0, y: 47, w: 6, h: 7 },
      [
        prom(
          'sum by (status) (manga_server_downloads_by_source_status{service_name="$job", source=~"$source"}) > 0',
          "{{status}}",
          "A",
          true,
          "table",
        ),
      ],
      "status",
      "short",
    ),
    barChartPanel(
      35,
      "Library manga by source",
      { x: 6, y: 47, w: 6, h: 7 },
      [
        prom(
          'manga_server_library_manga{service_name="$job", source=~"$source"} > 0',
          "{{source}}",
          "A",
          true,
          "table",
        ),
      ],
      "source",
      "short",
    ),
    piePanel(
      36,
      "Enabled plugins",
      { x: 12, y: 47, w: 6, h: 7 },
      [
        prom(
          'sum by (state) (manga_server_sources{service_name="$job", state=~"enabled|disabled"})',
          "{{state}}",
          "A",
          true,
        ),
      ],
      "short",
    ),
    gaugePanel(37, "Storage used", { x: 18, y: 47, w: 6, h: 7 }, storageUsedRatio),
    row(40, "Media, Background Work, and Resilience", 55),
    timeseriesPanel(
      41,
      "Media proxy activity",
      { x: 0, y: 56, w: 8, h: 8 },
      [
        prom(
          'sum by (outcome) (rate(manga_server_media_proxy_requests_total{service_name="$job", source=~"$source"}[$__rate_interval]))',
          "requests {{outcome}}",
        ),
        prom(
          'sum(rate(manga_server_media_proxy_bytes_total{service_name="$job", source=~"$source"}[$__rate_interval]))',
          "bytes/s",
          "B",
        ),
      ],
      "short",
    ),
    timeseriesPanel(
      42,
      "Media proxy stage duration",
      { x: 8, y: 56, w: 8, h: 8 },
      [prom(mediaProxyStageAvg, "avg {{stage}} {{outcome}}")],
      "s",
      3,
    ),
    timeseriesPanel(
      43,
      "Search warm work",
      { x: 16, y: 56, w: 8, h: 8 },
      [
        prom(
          'sum by (source, outcome) (rate(manga_server_search_cache_warm_queries_total{service_name="$job", source=~"$source"}[$__rate_interval]))',
          "search {{source}} {{outcome}}",
        ),
        prom(
          'sum by (outcome) (rate(manga_server_search_cache_warm_runs_total{service_name="$job"}[$__rate_interval]))',
          "run {{outcome}}",
          "B",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      44,
      "Public API rate limiter",
      { x: 0, y: 64, w: 8, h: 8 },
      [
        prom(
          'sum(rate(ratelimiter_calls_total{service_name="$job", ratelimiter="public-api", result="permitted"}[$__rate_interval])) or vector(0)',
          "permitted requests",
        ),
        prom(
          'sum(rate(ratelimiter_calls_total{service_name="$job", ratelimiter="public-api", result="rejected"}[$__rate_interval])) or vector(0)',
          "denied requests",
          "B",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      45,
      "Outbound retry activity",
      { x: 8, y: 64, w: 8, h: 8 },
      [
        prom(
          'sum by (result) (rate(retry_calls_total{service_name="$job", retry="plugin-outbound-http"}[$__rate_interval]))',
          "calls {{result}}",
        ),
        prom(
          'sum(rate(retry_attempts_count{service_name="$job", retry="plugin-outbound-http"}[$__rate_interval]))',
          "attempt observations",
          "B",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      46,
      "Discord notifications",
      { x: 16, y: 64, w: 8, h: 8 },
      [
        prom(
          'sum by (kind, outcome) (rate(manga_server_discord_notifications_total{service_name="$job"}[$__rate_interval]))',
          "{{kind}} {{outcome}}",
        ),
      ],
      "ops",
      2,
    ),
    timeseriesPanel(
      47,
      "Search warm duration",
      { x: 0, y: 72, w: 8, h: 8 },
      [prom(searchWarmDurationAvg, "avg {{outcome}}")],
      "s",
      3,
    ),
    timeseriesPanel(
      48,
      "Request coalescing",
      { x: 8, y: 72, w: 8, h: 8 },
      [
        prom(
          'sum by (role) (rate(coalesce_requests_total{service_name="$job", coalesce="plugin-outbound-http"}[$__rate_interval]))',
          "{{role}}",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      49,
      "Download storage bytes",
      { x: 16, y: 72, w: 8, h: 8 },
      [prom('manga_server_download_storage_bytes{service_name="$job"}', "{{kind}}")],
      "bytes",
    ),
    row(50, "Cache", 81),
    timeseriesPanel(
      51,
      "Foyer cache operations",
      { x: 0, y: 82, w: 8, h: 8 },
      [
        prom(
          'sum by (op) (rate(foyer_memory_op_total{service_name="$job", name=~"$cache"}[$__rate_interval]))',
          "memory {{op}}",
        ),
        prom(
          'sum by (op) (rate(foyer_storage_op_total{service_name="$job", name=~"$cache"}[$__rate_interval]))',
          "disk {{op}}",
          "B",
        ),
      ],
      "ops",
    ),
    timeseriesPanel(
      52,
      "Foyer memory and disk IO",
      { x: 8, y: 82, w: 8, h: 8 },
      [
        prom('foyer_memory_usage{service_name="$job", name=~"$cache"}', "memory {{name}}"),
        prom(
          'sum by (op) (rate(foyer_storage_disk_io_bytes_total{service_name="$job", name=~"$cache"}[$__rate_interval]))',
          "disk {{op}}",
          "B",
        ),
      ],
      "bytes",
    ),
    timeseriesPanel(
      53,
      "Foyer hit ratio",
      { x: 16, y: 82, w: 8, h: 8 },
      [prom(foyerHitRatio, "hybrid")],
      "percentunit",
      2,
    ),
    row(54, "Downloaded Archive Index", 91),
    timeseriesPanel(
      55,
      "Index request rate by result",
      { x: 0, y: 92, w: 8, h: 8 },
      [prom(archiveIndexRequestRate, "{{result}}")],
      "ops",
      2,
    ),
    timeseriesPanel(
      56,
      "Index build rate by trigger/outcome",
      { x: 8, y: 92, w: 8, h: 8 },
      [prom(archiveIndexBuildRate, "{{trigger}} {{outcome}}")],
      "ops",
      2,
    ),
    timeseriesPanel(
      57,
      "Index build duration avg",
      { x: 16, y: 92, w: 8, h: 8 },
      [prom(archiveIndexBuildAvg, "avg {{trigger}}")],
      "s",
      3,
    ),
    timeseriesPanel(
      58,
      "Indexed archives/pages/blob bytes",
      { x: 0, y: 100, w: 8, h: 8 },
      [
        prom(
          'manga_server_archive_index_entries{service_name="$job"} or on() vector(0)',
          "archives",
        ),
        prom(
          'manga_server_archive_index_pages{service_name="$job"} or on() vector(0)',
          "pages",
          "B",
        ),
        prom(
          'manga_server_archive_index_blob_bytes{service_name="$job"} or on() vector(0)',
          "blob bytes",
          "C",
        ),
      ],
      "short",
      0,
    ),
    timeseriesPanel(
      59,
      "Index cleanup rows by reason",
      { x: 8, y: 100, w: 8, h: 8 },
      [
        prom(
          'sum by (reason) (rate(manga_server_archive_index_cleanup_rows_total{service_name="$job"}[$__rate_interval])) or on() vector(0)',
          "{{reason}}",
        ),
      ],
      "ops",
      2,
    ),
    timeseriesPanel(
      60,
      "Reader p95 vs index rebuild/error rate",
      { x: 16, y: 100, w: 8, h: 8 },
      [
        prom(readerPageP95, "reader page p95"),
        prom(
          'sum(rate(manga_server_archive_index_builds_total{service_name="$job"}[$__rate_interval])) or on() vector(0)',
          "index builds/s",
          "B",
        ),
        prom(
          'sum(rate(manga_server_archive_index_requests_total{service_name="$job", result="error"}[$__rate_interval])) or on() vector(0)',
          "index errors/s",
          "C",
        ),
      ],
      "short",
      3,
    ),
    timeseriesPanel(
      61,
      "Downloaded page requests by bucket",
      { x: 0, y: 108, w: 8, h: 8 },
      [prom(downloadedPageRequestRate, "{{page_bucket}} {{outcome}}")],
      "ops",
      2,
    ),
    timeseriesPanel(
      62,
      "Downloaded page extraction avg",
      { x: 8, y: 108, w: 8, h: 8 },
      [prom(downloadedPageExtractAvg, "avg {{mode}} {{outcome}}")],
      "s",
      3,
    ),
    timeseriesPanel(
      63,
      "Downloaded page extraction pages/window",
      { x: 16, y: 108, w: 8, h: 8 },
      [
        prom(downloadedPageExtractPages, "pages/s {{mode}}"),
        prom(downloadedPageExtractWindowPages, "avg window pages {{mode}}", "B"),
      ],
      "short",
      2,
    ),
    row(64, "Downloaded Page Extraction Queues", 117),
    timeseriesPanel(
      65,
      "Extraction queue pressure",
      { x: 0, y: 118, w: 8, h: 8 },
      [
        prom(
          'manga_server_downloaded_page_extraction_scheduler_queue_depth{service_name="$job"} or on() vector(0)',
          "queue depth",
        ),
        prom(downloadedPageQueueWaitAvg, "avg wait {{outcome}}", "B"),
        prom(
          'sum by (reason) (rate(manga_server_downloaded_page_extraction_scheduler_backpressure_total{service_name="$job"}[$__rate_interval])) or on() vector(0)',
          "backpressure {{reason}}",
          "C",
        ),
      ],
      "short",
      3,
    ),
    timeseriesPanel(
      66,
      "Extraction coordinator batch size",
      { x: 8, y: 118, w: 8, h: 8 },
      [
        prom(downloadedPageBatchRequestsAvg, "avg requests"),
        prom(downloadedPageBatchArchivesAvg, "avg archives", "B"),
      ],
      "short",
      2,
    ),
    timeseriesPanel(
      67,
      "Extraction worker group size",
      { x: 16, y: 118, w: 8, h: 8 },
      [
        prom(downloadedPageGroupRequestsAvg, "avg requests"),
        prom(downloadedPageGroupPagesAvg, "avg pages", "B"),
      ],
      "short",
      2,
    ),
    timeseriesPanel(
      68,
      "Extraction worker and response duration",
      { x: 0, y: 126, w: 12, h: 8 },
      [
        prom(downloadedPageWorkerAvg, "worker {{outcome}}"),
        prom(downloadedPageResponseBuildAvg, "response build {{mode}}", "B"),
      ],
      "s",
      3,
    ),
    row(70, "Runtime", 135),
    timeseriesPanel(
      71,
      "Runtime pressure",
      { x: 0, y: 136, w: 12, h: 8 },
      [
        prom('manga_server_tokio_live_tasks_count{service_name="$job"}', "live tasks"),
        prom('manga_server_tokio_global_queue_depth{service_name="$job"}', "global queue", "B"),
        prom('manga_server_tokio_total_local_queue_depth{service_name="$job"}', "local queue", "C"),
        prom('manga_server_tokio_blocking_queue_depth{service_name="$job"}', "blocking queue", "D"),
      ],
      "short",
      0,
    ),
    timeseriesPanel(
      72,
      "Runtime busy and poll cost",
      { x: 12, y: 136, w: 12, h: 8 },
      [
        prom('manga_server_tokio_busy_ratio{service_name="$job"}', "busy ratio"),
        prom('manga_server_tokio_mean_poll_duration{service_name="$job"}', "mean poll ns", "B"),
      ],
      "short",
      3,
    ),
    row(80, "Alerts and Runbook", 145),
    alertListPanel(81, "Grafana alert state", { x: 0, y: 146, w: 8, h: 7 }),
    tablePanel(
      82,
      "Metrics alert state",
      { x: 8, y: 146, w: 8, h: 7 },
      [
        prom(
          'ALERTS{alertname=~"MangaServer.*"} or vector(0)',
          "{{alertname}} {{alertstate}}",
          "A",
          true,
          "table",
        ),
      ],
      "short",
    ),
    textPanel(
      83,
      "Runbook links",
      { x: 16, y: 146, w: 8, h: 7 },
      "- OTLP HTTP: `http://localhost:4318`\n- OTLP gRPC: `http://localhost:4317`\n- Grafana: `http://localhost:3000`\n- Logs/traces dashboard: `/d/manga-server-signals`\n- Tokio runtime dashboard: `/d/manga-server-tokio-runtime`",
    ),
    row(90, "Source/API Signals", 154),
    timeseriesPanel(
      91,
      "Source operation rate",
      { x: 0, y: 155, w: 8, h: 8 },
      [prom(sourceOperationRate, "{{source}} {{operation}} {{outcome}} {{cache_result}}")],
      "ops",
      2,
    ),
    timeseriesPanel(
      92,
      "Source operation duration avg",
      { x: 8, y: 155, w: 8, h: 8 },
      [prom(sourceOperationDurationAvg, "{{source}} {{operation}} {{outcome}} {{cache_result}}")],
      "s",
      3,
    ),
    timeseriesPanel(
      93,
      "Source items returned",
      { x: 16, y: 155, w: 8, h: 8 },
      [prom(sourceOperationItemsRate, "{{source}} {{operation}}")],
      "ops",
      2,
    ),
    timeseriesPanel(
      94,
      "Source plugin changes",
      { x: 0, y: 163, w: 12, h: 8 },
      [prom(sourcePluginChanges, "{{source}} {{operation}} {{outcome}}")],
      "short",
      0,
    ),
    timeseriesPanel(
      95,
      "API error rate",
      { x: 12, y: 163, w: 12, h: 8 },
      [prom(requestErrorRate, "{{status_code}} {{code}} {{outcome}}")],
      "ops",
      2,
    ),
  ].forEach((panel) =>
    panel instanceof dashboard.RowBuilder ? builder.withRow(panel) : builder.withPanel(panel),
  );

  return builder.build();
}
