export function mangaServerOverviewSignals() {
  return {
    downloadWorkState: {
      archiveBytesAvg:
        '(sum by (source) (rate(manga_server_download_archive_bytes_sum{service_name="$job", source=~"$source"}[$__rate_interval])) / clamp_min(sum by (source) (rate(manga_server_download_archive_bytes_count{service_name="$job", source=~"$source"}[$__rate_interval])), 0.001)) or on() vector(0)',
      durationP95:
        'manga_server_download_duration_seconds{service_name="$job", source=~"$source", quantile="0.95"} or on() vector(0)',
      pageFetchP95:
        'manga_server_download_page_fetch_duration_seconds{service_name="$job", source=~"$source", quantile="0.95"} or on() vector(0)',
      storageUsedRatio:
        '(sum(manga_server_download_storage_bytes{service_name="$job", kind="used"}) / clamp_min(sum(manga_server_download_storage_bytes{service_name="$job", kind="limit"}), 1)) or on() vector(0)',
    },
    downloadedArchiveIndex: {
      buildAvg:
        '(sum by (trigger) (rate(manga_server_archive_index_build_duration_seconds_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum by (trigger) (rate(manga_server_archive_index_build_duration_seconds_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)',
      buildRate:
        'sum by (trigger, outcome) (rate(manga_server_archive_index_builds_total{service_name="$job"}[$__rate_interval])) or on() vector(0)',
      cleanupRows:
        'sum by (reason) (rate(manga_server_archive_index_cleanup_rows_total{service_name="$job"}[$__rate_interval])) or on() vector(0)',
      requestRate:
        'sum by (result) (rate(manga_server_archive_index_requests_total{service_name="$job"}[$__rate_interval])) or on() vector(0)',
    },
    downloadedChapterPage: {
      extractAvg:
        '(sum by (mode, outcome) (rate(manga_server_downloaded_page_extract_duration_seconds_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum by (mode, outcome) (rate(manga_server_downloaded_page_extract_duration_seconds_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)',
      extractPages:
        'sum by (mode) (rate(manga_server_downloaded_page_extract_pages_total{service_name="$job"}[$__rate_interval])) or on() vector(0)',
      extractWindowPages:
        '(sum by (mode) (rate(manga_server_downloaded_page_extract_window_pages_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum by (mode) (rate(manga_server_downloaded_page_extract_window_pages_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)',
      readerPageP95:
        'histogram_quantile(0.95, sum by (le) (rate({__name__="http.server.request.duration_bucket", service_name="$job", "http.route"=~".*/pages.*"}[$__rate_interval]))) or on() vector(0)',
      requestRate:
        'sum by (page_bucket, outcome) (rate(manga_server_downloaded_page_requests_total{service_name="$job"}[$__rate_interval])) or on() vector(0)',
      responseBuildAvg:
        '(sum by (mode) (rate(manga_server_downloaded_page_response_build_duration_seconds_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum by (mode) (rate(manga_server_downloaded_page_response_build_duration_seconds_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)',
    },
    downloadedChapterPageExtractionScheduler: {
      batchArchivesAvg:
        '(sum(rate(manga_server_downloaded_page_extraction_scheduler_batch_archives_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum(rate(manga_server_downloaded_page_extraction_scheduler_batch_archives_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)',
      batchRequestsAvg:
        '(sum(rate(manga_server_downloaded_page_extraction_scheduler_batch_requests_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum(rate(manga_server_downloaded_page_extraction_scheduler_batch_requests_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)',
      groupPagesAvg:
        '(sum(rate(manga_server_downloaded_page_extraction_scheduler_group_pages_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum(rate(manga_server_downloaded_page_extraction_scheduler_group_pages_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)',
      groupRequestsAvg:
        '(sum(rate(manga_server_downloaded_page_extraction_scheduler_group_requests_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum(rate(manga_server_downloaded_page_extraction_scheduler_group_requests_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)',
      queueWaitAvg:
        '(sum by (outcome) (rate(manga_server_downloaded_page_extraction_scheduler_queue_wait_seconds_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum by (outcome) (rate(manga_server_downloaded_page_extraction_scheduler_queue_wait_seconds_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)',
      workerAvg:
        '(sum by (outcome) (rate(manga_server_downloaded_page_extraction_scheduler_worker_duration_seconds_sum{service_name="$job"}[$__rate_interval])) / clamp_min(sum by (outcome) (rate(manga_server_downloaded_page_extraction_scheduler_worker_duration_seconds_count{service_name="$job"}[$__rate_interval])), 0.001)) or on() vector(0)',
    },
    sourceCatalog: {
      operationDurationAvg:
        '(sum by (source, operation, outcome, cache_result) (rate(manga_server_source_operation_duration_seconds_sum{service_name="$job", source=~"$source"}[$__rate_interval])) / clamp_min(sum by (source, operation, outcome, cache_result) (rate(manga_server_source_operation_duration_seconds_count{service_name="$job", source=~"$source"}[$__rate_interval])), 0.001)) or on() vector(0)',
      operationItemsRate:
        'sum by (source, operation) (rate(manga_server_source_operation_items_total{service_name="$job", source=~"$source"}[$__rate_interval])) or on() vector(0)',
      operationRate:
        'sum by (source, operation, outcome, cache_result) (rate(manga_server_source_operations_total{service_name="$job", source=~"$source"}[$__rate_interval])) or on() vector(0)',
      pluginChanges:
        'sum by (source, action, outcome) (increase(manga_server_source_plugin_changes_total{service_name="$job", source=~"$source"}[$__range])) or on() vector(0)',
    },
  } as const;
}
