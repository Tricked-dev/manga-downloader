import assert from "node:assert/strict";
import { join } from "node:path";

import { observabilityArtifacts } from "./generate.js";
import { mangaServerOverviewSignals } from "./telemetry-signals.js";

const root = "/tmp/manga-observability-test";
const artifacts = observabilityArtifacts(root);
const paths = artifacts.map((artifact) => artifact.path);

assert.equal(artifacts.filter((artifact) => artifact.format === "json").length, 5);
assert.equal(artifacts.filter((artifact) => artifact.format === "yaml").length, 5);
assert.equal(new Set(paths).size, paths.length);
assert(paths.includes(join(root, "grafana/dashboards/manga-server-overview.json")));
assert(paths.includes(join(root, "grafana/provisioning/datasources/datasources.yaml")));
assert(paths.includes(join(root, "docker-compose.yml")));

const signals = mangaServerOverviewSignals();
assert(
  signals.downloadedArchiveIndex.requestRate.includes("manga_server_archive_index_requests_total"),
);
assert(
  signals.downloadedChapterPageExtractionScheduler.queueWaitAvg.includes(
    "manga_server_downloaded_page_extraction_scheduler_queue_wait_seconds",
  ),
);
assert(signals.sourceCatalog.operationRate.includes("manga_server_source_operations_total"));
