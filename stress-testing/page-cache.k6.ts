import exec from "k6/execution";
import { check } from "k6";
import { Counter, Rate, Trend } from "k6/metrics";
import {
  DOWNLOADED_PAGE_ROUTE,
  DownloadedPageTarget,
  ResponseLike,
  SummaryData,
  TestConfig,
  clearCache,
  closedModelScenario,
  discoverDownloadedChapters,
  downloadedPageTargets,
  imageGet,
  imageResponseOk,
  loadConfig,
  makeSummary,
  metricValue,
  targetByIteration,
  trendSummaryLine,
} from "./lib/manga.ts";

type PageCacheMode = "cached" | "uncached" | "clear-cache";

type SetupData = {
  config: TestConfig;
  mode: PageCacheMode;
  targets: DownloadedPageTarget[];
};

const mode = loadMode();
const baseConfig = loadConfig();
const config = {
  ...baseConfig,
  skipPageCache: mode === "uncached" ? true : baseConfig.skipPageCache,
};
const pageOk = new Rate("manga_k6_page_cache_ok");
const pageResponseTime = new Trend("manga_k6_page_cache_response_time", true);
const warmedPages = new Counter("manga_k6_page_cache_warmed_total");
const cacheClearOk = new Rate("manga_k6_page_cache_clear_ok");
const cacheClears = new Counter("manga_k6_page_cache_clears_total");
let lastCacheClearAt = 0;

export const options = {
  scenarios: {
    page_cache: closedModelScenario(config, "page_cache", { mode }),
  },
  thresholds: {
    http_req_failed: ["rate<0.01"],
    "http_req_duration{endpoint:downloaded_page}": [config.p95Threshold, config.p99Threshold],
    manga_k6_page_cache_ok: ["rate>0.99"],
  },
  summaryTrendStats: ["avg", "min", "med", "max", "p(90)", "p(95)", "p(99)"],
  userAgent: "manga-downloader-k6/2.0",
};

export function setup(): SetupData {
  if (mode === "uncached" || mode === "clear-cache") {
    const ok = clearCache(config);
    cacheClearOk.add(ok);
    cacheClears.add(1);
    if (!ok) {
      throw new Error("Initial cache clear failed; refusing to run page-cache test");
    }
  }

  const chapters = discoverDownloadedChapters(config);
  const targetCount = config.urlPool > 0 ? config.urlPool : config.requests;
  const targetMode = mode === "cached" ? "random" : "unique";
  const targets = downloadedPageTargets(config, chapters, targetCount, targetMode);
  if (targets.length === 0) {
    throw new Error("No downloaded page targets were discovered");
  }

  if (mode === "cached") {
    for (const target of targets) {
      const response = requestPage(config, target);
      if (imageResponseOk(response)) {
        warmedPages.add(1);
      }
    }
  }

  console.log(`page_cache mode=${mode} targets=${targets.length}`);
  return { config, mode, targets };
}

export default function (data: SetupData): void {
  maybeClearCache(data.config, data.mode);

  const target = targetByIteration(data.targets, exec.scenario.iterationInTest);
  const response = requestPage(data.config, target);
  const ok = check(response, {
    "downloaded page status/content-type is valid": (res) => imageResponseOk(res as ResponseLike),
  });

  pageOk.add(ok, { mode: data.mode });
  pageResponseTime.add(response.timings.duration, { mode: data.mode });
}

export function handleSummary(data: SummaryData) {
  const cacheClearCount = metricValue(data.metrics?.manga_k6_page_cache_clears_total, "count");
  return makeSummary(`${mode} downloaded-page cache`, config, data, [
    trendSummaryLine("downloaded_page", data.metrics?.manga_k6_page_cache_response_time),
    `  mode: ${mode}`,
    `  skip_page_cache: ${config.skipPageCache}`,
    `  cache_clears: ${cacheClearCount.toFixed(0)}`,
  ]);
}

function requestPage(value: TestConfig, target: DownloadedPageTarget): ResponseLike {
  return imageGet(value, target.url, "downloaded_page", DOWNLOADED_PAGE_ROUTE, {
    mode,
  });
}

function maybeClearCache(value: TestConfig, currentMode: PageCacheMode): void {
  if (
    currentMode !== "clear-cache" ||
    value.cacheClearEverySeconds <= 0 ||
    exec.vu.idInTest !== 1
  ) {
    return;
  }

  const now = Date.now();
  if (now - lastCacheClearAt < value.cacheClearEverySeconds * 1000) {
    return;
  }

  lastCacheClearAt = now;
  const ok = clearCache(value);
  cacheClearOk.add(ok);
  if (ok) {
    cacheClears.add(1);
  }
}

function loadMode(): PageCacheMode {
  const raw = String(__ENV.PAGE_CACHE_MODE || "")
    .trim()
    .toLowerCase();
  if (raw === "uncached" || raw === "clear-cache") {
    return raw;
  }
  return "cached";
}
