import http from "k6/http";

export type TestConfig = {
  server: string;
  apiKey: string;
  requests: number;
  concurrency: number;
  duration: string;
  chapters: number;
  urlPool: number;
  pagesPerJourney: number;
  timeout: string;
  summaryPath: string;
  p95Threshold: string;
  p99Threshold: string;
  skipPageCache: boolean;
  cacheClearEverySeconds: number;
  thinkMinSeconds: number;
  thinkMaxSeconds: number;
};

export type WorkloadScenario = {
  executor: string;
  vus?: number;
  iterations?: number;
  duration?: string;
  maxDuration?: string;
  gracefulStop?: string;
  tags?: Record<string, string>;
};

export type SummaryMetric = {
  values?: Record<string, number>;
};

export type SummaryData = {
  metrics?: Record<string, SummaryMetric>;
};

export type SummaryOutput = Record<string, string>;

export type ResponseLike = {
  status: number;
  headers: Record<string, string>;
  timings: { duration: number };
  json(): unknown;
};

export type LibraryManga = {
  id: string;
  title: string;
  source: string;
  sourceBaseUrl: string;
  coverUrl: string;
  coverProxyUrl: string;
};

export type DownloadedChapter = {
  id: string;
  mangaId: string;
  title: string;
  chapterNumber: string;
  pages: number;
};

export type DownloadedPageTarget = {
  chapterId: string;
  page: number;
  url: string;
};

export type MediaProxyTarget = {
  title: string;
  source: string;
  url: string;
};

export type MediaProxyOptions = {
  format: string;
  width: number;
  skipCache: boolean;
};

type ApiListResponse<T> = {
  items?: T[];
};

type LibraryMangaItem = {
  id?: string;
  title?: string;
  source?: string;
  source_base_url?: string | null;
  cover_url?: string;
  cover_proxy_url?: string | null;
};

type ChapterItem = {
  id?: string;
  manga_id?: string;
  title?: string;
  chapter_number?: string | number;
  downloaded?: boolean;
};

const DEFAULT_SERVER = "http://127.0.0.1:4000";
const DEFAULT_REQUESTS = 1000;
const DEFAULT_CONCURRENCY = 20;
const DEFAULT_TIMEOUT = "10s";

export const DOWNLOADED_PAGE_ROUTE = "/v1/library/chapters/:chapter_id/pages/:page";
export const DOWNLOADED_PAGE_LIST_ROUTE = "/v1/library/chapters/:chapter_id/pages";
export const MEDIA_PROXY_IMAGE_ROUTE = "/v1/media/image";

export function loadConfig(): TestConfig {
  return {
    server: normalizeServer(__ENV.SERVER || DEFAULT_SERVER),
    apiKey: normalizeApiKey(__ENV.BACKEND_API_KEY || __ENV.API_KEY || ""),
    requests: positiveInt(__ENV.REQUESTS, DEFAULT_REQUESTS),
    concurrency: positiveInt(__ENV.CONCURRENCY, DEFAULT_CONCURRENCY),
    duration: stringOrEmpty(__ENV.DURATION),
    chapters: nonNegativeInt(__ENV.CHAPTERS, 0),
    urlPool: nonNegativeInt(__ENV.URL_POOL, 0),
    pagesPerJourney: positiveInt(__ENV.PAGES_PER_JOURNEY, 3),
    timeout: __ENV.TIMEOUT || DEFAULT_TIMEOUT,
    summaryPath: stringOrEmpty(__ENV.SUMMARY_PATH),
    p95Threshold: __ENV.P95_THRESHOLD || "p(95)<100",
    p99Threshold: __ENV.P99_THRESHOLD || "p(99)<250",
    skipPageCache: booleanFlag(__ENV.SKIP_PAGE_CACHE, false),
    cacheClearEverySeconds: nonNegativeNumber(__ENV.CACHE_CLEAR_EVERY_SECONDS, 0),
    thinkMinSeconds: nonNegativeNumber(__ENV.THINK_MIN_SECONDS, 0),
    thinkMaxSeconds: nonNegativeNumber(__ENV.THINK_MAX_SECONDS, 0),
  };
}

export function closedModelScenario(
  config: TestConfig,
  workload: string,
  extraTags: Record<string, string> = {},
): WorkloadScenario {
  const tags = { workload, ...extraTags };
  if (config.duration) {
    return {
      executor: "constant-vus",
      vus: config.concurrency,
      duration: config.duration,
      gracefulStop: "5s",
      tags,
    };
  }

  return {
    executor: "shared-iterations",
    vus: config.concurrency,
    iterations: config.requests,
    maxDuration: "10m",
    tags,
  };
}

export function requestHeaders(
  config: TestConfig,
  accept = "application/json",
): Record<string, string> {
  const headers: Record<string, string> = { Accept: accept };
  if (config.apiKey) {
    headers.Authorization = `Bearer ${config.apiKey}`;
  }
  return headers;
}

export function apiGet(
  config: TestConfig,
  path: string,
  endpoint: string,
  tags: Record<string, string> = {},
): ResponseLike {
  return http.get(`${config.server}${path}`, {
    headers: requestHeaders(config),
    timeout: config.timeout,
    tags: { endpoint, name: requestNameForPath(path), ...tags },
  }) as ResponseLike;
}

export function apiPost(config: TestConfig, path: string, endpoint: string): ResponseLike {
  return http.post(`${config.server}${path}`, "", {
    headers: requestHeaders(config),
    timeout: config.timeout,
    tags: { endpoint, name: requestNameForPath(path) },
  }) as ResponseLike;
}

export function imageGet(
  config: TestConfig,
  url: string,
  endpoint: string,
  name: string,
  tags: Record<string, string> = {},
): ResponseLike {
  return http.get(url, {
    headers: requestHeaders(config, "image/avif,image/webp,image/*,*/*"),
    timeout: config.timeout,
    responseType: "binary",
    tags: { endpoint, name, ...tags },
  }) as ResponseLike;
}

export function clearCache(config: TestConfig): boolean {
  const response = apiPost(config, "/v1/settings/clear-cache", "clear_cache");
  return response.status >= 200 && response.status < 300;
}

export function discoverLibrary(config: TestConfig): LibraryManga[] {
  const response = apiGet(config, "/v1/library", "discovery");
  if (!jsonResponseOk(response)) {
    throw new Error(`Failed to fetch library manga: HTTP ${response.status}`);
  }

  return ((response.json() as ApiListResponse<LibraryMangaItem>).items || [])
    .map((manga) => ({
      id: manga.id || "",
      title: manga.title || "untitled",
      source: manga.source || "unscoped",
      sourceBaseUrl: manga.source_base_url || "",
      coverUrl: manga.cover_url || "",
      coverProxyUrl: manga.cover_proxy_url || "",
    }))
    .filter((manga) => manga.id.length > 0);
}

export function discoverDownloadedChapters(config: TestConfig): DownloadedChapter[] {
  const response = apiGet(config, "/v1/library/chapters", "discovery");
  if (!jsonResponseOk(response)) {
    throw new Error(`Failed to fetch downloaded chapters: HTTP ${response.status}`);
  }

  let chapters = ((response.json() as ApiListResponse<ChapterItem>).items || [])
    .filter((chapter) => chapter.downloaded === true && typeof chapter.id === "string")
    .sort(() => Math.random() - 0.5);

  if (config.chapters > 0) {
    chapters = chapters.slice(0, config.chapters);
  }

  return chapters
    .map((chapter) => {
      const pages = discoverChapterPageCount(config, chapter.id || "");
      if (pages === 0) {
        return null;
      }

      return {
        id: chapter.id || "",
        mangaId: chapter.manga_id || "",
        title: chapter.title || "",
        chapterNumber: String(chapter.chapter_number || ""),
        pages,
      };
    })
    .filter((chapter): chapter is DownloadedChapter => chapter !== null);
}

export function discoverChapterPageCount(config: TestConfig, chapterId: string): number {
  const response = apiGet(config, downloadedPageListPath(chapterId, true), "discovery");
  if (!jsonResponseOk(response)) {
    console.warn(`Skipping ${chapterId}: pages request returned HTTP ${response.status}`);
    return 0;
  }

  const body = response.json() as ApiListResponse<string>;
  return (body.items || []).length;
}

export function downloadedPageTargets(
  config: TestConfig,
  chapters: DownloadedChapter[],
  limit: number,
  mode: "random" | "reader" | "unique",
): DownloadedPageTarget[] {
  if (chapters.length === 0 || limit <= 0) {
    return [];
  }

  if (mode === "unique") {
    return shuffle(
      chapters.flatMap((chapter) =>
        range(0, chapter.pages).map((page) => downloadedPageTarget(config, chapter, page)),
      ),
    ).slice(0, limit);
  }

  if (mode === "reader") {
    return range(0, limit).map((requestIndex) => {
      const chapter = chapters[requestIndex % chapters.length];
      return downloadedPageTarget(config, chapter, requestIndex % chapter.pages);
    });
  }

  return range(0, limit).map(() => {
    const chapter = chapters[randomInt(0, chapters.length - 1)];
    return downloadedPageTarget(config, chapter, randomInt(0, chapter.pages - 1));
  });
}

export function mediaProxyTargets(
  config: TestConfig,
  manga: LibraryManga[],
  limit: number,
  options: MediaProxyOptions,
): MediaProxyTarget[] {
  const targets = shuffle(
    manga
      .map((item) => mediaProxyTargetFor(config, item, options))
      .filter((target): target is MediaProxyTarget => target !== null),
  );

  return limit > 0 ? targets.slice(0, limit) : targets;
}

export function downloadedPageListPath(chapterId: string, skipCache: boolean): string {
  return `/v1/library/chapters/${encodeURIComponent(chapterId)}/pages${skipCache ? "?skip_page_cache=true" : ""}`;
}

export function imageResponseOk(response: ResponseLike): boolean {
  const contentType = String(
    response.headers["Content-Type"] || response.headers["content-type"] || "",
  );
  return response.status === 200 && contentType.startsWith("image/");
}

export function jsonResponseOk(response: ResponseLike): boolean {
  const contentType = String(
    response.headers["Content-Type"] || response.headers["content-type"] || "",
  );
  return (
    response.status >= 200 && response.status < 300 && contentType.includes("application/json")
  );
}

export function targetByIteration<T>(targets: T[], iteration: number): T {
  if (targets.length === 0) {
    throw new Error("No targets were prepared for this workload");
  }
  return targets[iteration % targets.length];
}

export function randomThinkSeconds(config: TestConfig): number {
  if (config.thinkMaxSeconds <= 0) {
    return 0;
  }
  const min = Math.min(config.thinkMinSeconds, config.thinkMaxSeconds);
  const max = Math.max(config.thinkMinSeconds, config.thinkMaxSeconds);
  return min + Math.random() * (max - min);
}

export function makeSummary(
  name: string,
  config: TestConfig,
  data: SummaryData,
  extraLines: string[] = [],
): SummaryOutput {
  const output: SummaryOutput = {
    stdout: shortSummary(name, data, extraLines),
  };
  if (config.summaryPath) {
    output[config.summaryPath] = JSON.stringify(data, null, 2);
  }
  return output;
}

export function metricValue(metric: SummaryMetric | undefined, name: string): number {
  return metric?.values?.[name] || 0;
}

export function trendSummaryLine(label: string, metric: SummaryMetric | undefined): string {
  const values = metric?.values || {};
  return `  ${label}: avg=${formatNumber(values.avg, 2)}ms p95=${formatNumber(values["p(95)"], 2)}ms p99=${formatNumber(values["p(99)"], 2)}ms`;
}

function shortSummary(name: string, data: SummaryData, extraLines: string[]): string {
  const metrics = data.metrics || {};
  const httpReqs = metricValue(metrics.http_reqs, "count");
  const failedRate = metricValue(metrics.http_req_failed, "rate");
  const duration = metrics.http_req_duration?.values || {};

  return [
    "",
    `${name} k6 summary`,
    `  requests: ${formatNumber(httpReqs, 0)}`,
    `  failed: ${(failedRate * 100).toFixed(2)}%`,
    `  avg: ${formatNumber(duration.avg, 2)} ms`,
    `  p50: ${formatNumber(duration.med, 2)} ms`,
    `  p95: ${formatNumber(duration["p(95)"], 2)} ms`,
    `  p99: ${formatNumber(duration["p(99)"], 2)} ms`,
    ...extraLines,
    "",
  ].join("\n");
}

function downloadedPageTarget(
  config: TestConfig,
  chapter: DownloadedChapter,
  page: number,
): DownloadedPageTarget {
  const query = config.skipPageCache ? "?skip_page_cache=true" : "";
  return {
    chapterId: chapter.id,
    page,
    url: `${config.server}/v1/library/chapters/${encodeURIComponent(chapter.id)}/pages/${page}${query}`,
  };
}

function mediaProxyTargetFor(
  config: TestConfig,
  manga: LibraryManga,
  options: MediaProxyOptions,
): MediaProxyTarget | null {
  let path = manga.coverProxyUrl;

  if (!path && manga.coverUrl) {
    if (manga.coverUrl.startsWith(MEDIA_PROXY_IMAGE_ROUTE)) {
      path = manga.coverUrl;
    } else {
      const sourceUrl = normalizeAbsoluteMediaUrl(manga.coverUrl, manga.sourceBaseUrl);
      if (!sourceUrl) {
        return null;
      }

      path = `${MEDIA_PROXY_IMAGE_ROUTE}?${encodeQuery([
        ["source", manga.source],
        ["url", sourceUrl],
      ])}`;
    }
  }

  if (!path) {
    return null;
  }

  return {
    title: manga.title,
    source: manga.source,
    url: resolveBenchmarkUrl(config, appendMediaProxyOptions(path, options)),
  };
}

function appendMediaProxyOptions(url: string, options: MediaProxyOptions): string {
  const params: string[][] = [];
  if (options.format) {
    params.push(["format", options.format]);
  }
  if (options.width > 0) {
    params.push(["width", Math.round(options.width).toString()]);
  }
  if (options.skipCache) {
    params.push(["skip_cache", "true"]);
  }

  const query = encodeQuery(params);
  return query ? `${url}${url.includes("?") ? "&" : "?"}${query}` : url;
}

function requestNameForPath(path: string): string {
  const normalized = `/${String(path).replace(/^\/+/, "").split("?")[0]}`;
  if (normalized.startsWith(MEDIA_PROXY_IMAGE_ROUTE)) {
    return MEDIA_PROXY_IMAGE_ROUTE;
  }
  if (/^\/v1\/library\/chapters\/[^/]+\/pages\/\d+$/.test(normalized)) {
    return DOWNLOADED_PAGE_ROUTE;
  }
  if (/^\/v1\/library\/chapters\/[^/]+\/pages$/.test(normalized)) {
    return DOWNLOADED_PAGE_LIST_ROUTE;
  }
  return normalized;
}

function normalizeServer(server: string): string {
  const trimmed = String(server).trim().replace(/\/+$/, "");
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
    return trimmed;
  }
  return `http://${trimmed}`;
}

function normalizeApiKey(apiKey: string): string {
  return String(apiKey || "").trim();
}

function stringOrEmpty(value: string | undefined): string {
  return value ? String(value).trim() : "";
}

function booleanFlag(value: string | undefined, fallback: boolean): boolean {
  if (value === undefined || String(value).trim() === "") {
    return fallback;
  }
  return ["1", "true", "yes", "on"].includes(String(value).trim().toLowerCase());
}

function positiveInt(value: string | undefined, fallback: number): number {
  const parsed = Number.parseInt(value || "", 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function nonNegativeInt(value: string | undefined, fallback: number): number {
  const parsed = Number.parseInt(value || "", 10);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
}

function nonNegativeNumber(value: string | undefined, fallback: number): number {
  const parsed = Number.parseFloat(value || "");
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
}

function formatNumber(value: number | undefined, digits: number): string {
  if (!Number.isFinite(value)) {
    return "0";
  }
  return value.toFixed(digits);
}

function randomInt(min: number, max: number): number {
  return Math.floor(Math.random() * (max - min + 1)) + min;
}

function range(start: number, endExclusive: number): number[] {
  const values = [];
  for (let value = start; value < endExclusive; value += 1) {
    values.push(value);
  }
  return values;
}

function shuffle<T>(items: T[]): T[] {
  const copy = [...items];
  for (let index = copy.length - 1; index > 0; index -= 1) {
    const swapIndex = randomInt(0, index);
    [copy[index], copy[swapIndex]] = [copy[swapIndex], copy[index]];
  }
  return copy;
}

function encodeQuery(params: string[][]): string {
  return params
    .filter(([, value]) => value.length > 0)
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`)
    .join("&");
}

function resolveBenchmarkUrl(config: TestConfig, path: string): string {
  if (path.startsWith("http://") || path.startsWith("https://")) {
    return path;
  }
  return `${config.server}/${path.replace(/^\/+/, "")}`;
}

function normalizeAbsoluteMediaUrl(url: string, fallbackBaseUrl: string): string {
  const trimmedUrl = url.trim();
  if (trimmedUrl.startsWith("//")) {
    return `https:${trimmedUrl}`;
  }
  if (trimmedUrl.startsWith("/")) {
    return fallbackBaseUrl ? `${fallbackBaseUrl.replace(/\/$/, "")}${trimmedUrl}` : "";
  }
  if (!/^https?:\/\//i.test(trimmedUrl)) {
    return `https://${trimmedUrl}`;
  }
  return trimmedUrl;
}
