import exec from "k6/execution";
import { check } from "k6";
import { Counter, Rate, Trend } from "k6/metrics";
import {
  MEDIA_PROXY_IMAGE_ROUTE,
  MediaProxyOptions,
  MediaProxyTarget,
  ResponseLike,
  SummaryData,
  TestConfig,
  closedModelScenario,
  discoverLibrary,
  imageGet,
  imageResponseOk,
  loadConfig,
  makeSummary,
  mediaProxyTargets,
  targetByIteration,
  trendSummaryLine,
} from "./lib/manga.ts";

type MediaProxyMode = "cached" | "uncached";

type MediaProxyConfig = {
  mode: MediaProxyMode;
  format: string;
  width: number;
  warmup: boolean;
  p95Threshold: string;
  p99Threshold: string;
};

type SetupData = {
  config: TestConfig;
  media: MediaProxyConfig;
  targets: MediaProxyTarget[];
};

const config = loadConfig();
const mediaConfig = loadMediaProxyConfig();
const mediaProxyOk = new Rate("manga_k6_media_proxy_ok");
const mediaProxyCacheHit = new Rate("manga_k6_media_proxy_cache_hit");
const mediaProxyResponseTime = new Trend("manga_k6_media_proxy_response_time", true);
const warmedImages = new Counter("manga_k6_media_proxy_warmed_total");

export const options = {
  scenarios: {
    media_proxy: closedModelScenario(config, "media_proxy", { mode: mediaConfig.mode }),
  },
  thresholds: {
    http_req_failed: ["rate<0.01"],
    "http_req_duration{endpoint:media_proxy_image}": [
      mediaConfig.p95Threshold,
      mediaConfig.p99Threshold,
    ],
    manga_k6_media_proxy_ok: ["rate>0.99"],
  },
  summaryTrendStats: ["avg", "min", "med", "max", "p(90)", "p(95)", "p(99)"],
  userAgent: "manga-downloader-k6/2.0",
};

export function setup(): SetupData {
  const library = discoverLibrary(config);
  const targetCount = config.urlPool > 0 ? config.urlPool : Math.min(config.requests, 1000);
  const targets = mediaProxyTargets(config, library, targetCount, mediaProxyOptions(mediaConfig));
  if (targets.length === 0) {
    throw new Error("No media proxy targets were discovered from library covers");
  }

  if (mediaConfig.warmup) {
    for (const target of targets) {
      const response = requestMediaProxyImage(config, target);
      if (imageResponseOk(response)) {
        warmedImages.add(1);
      }
    }
  }

  console.log(
    `media_proxy mode=${mediaConfig.mode} targets=${targets.length} format=${mediaConfig.format || "original"} width=${mediaConfig.width || "original"} warmup=${mediaConfig.warmup}`,
  );
  return { config, media: mediaConfig, targets };
}

export default function (data: SetupData): void {
  const target = targetByIteration(data.targets, exec.scenario.iterationInTest);
  const response = requestMediaProxyImage(data.config, target);
  const ok = check(response, {
    "media proxy status/content-type is valid": (res) => imageResponseOk(res as ResponseLike),
  });

  mediaProxyOk.add(ok, { mode: data.media.mode });
  mediaProxyCacheHit.add(imageCacheHeader(response) === "HIT", { mode: data.media.mode });
  mediaProxyResponseTime.add(response.timings.duration, { mode: data.media.mode });
}

export function handleSummary(data: SummaryData) {
  return makeSummary("media proxy image reading", config, data, [
    trendSummaryLine("media_proxy", data.metrics?.manga_k6_media_proxy_response_time),
    `  mode: ${mediaConfig.mode}`,
    `  format: ${mediaConfig.format || "original"}`,
    `  width: ${mediaConfig.width || "original"}`,
    `  warmup: ${mediaConfig.warmup}`,
  ]);
}

function requestMediaProxyImage(value: TestConfig, target: MediaProxyTarget): ResponseLike {
  return imageGet(value, target.url, "media_proxy_image", MEDIA_PROXY_IMAGE_ROUTE, {
    source: target.source,
    mode: mediaConfig.mode,
  });
}

function mediaProxyOptions(value: MediaProxyConfig): MediaProxyOptions {
  return {
    format: value.format,
    width: value.width,
    skipCache: value.mode === "uncached",
  };
}

function imageCacheHeader(response: ResponseLike): string {
  return String(response.headers["X-Image-Cache"] || response.headers["x-image-cache"] || "");
}

function loadMediaProxyConfig(): MediaProxyConfig {
  const mode =
    String(__ENV.MEDIA_PROXY_MODE || "")
      .trim()
      .toLowerCase() === "uncached"
      ? "uncached"
      : "cached";
  return {
    mode,
    format: stringOrEmpty(__ENV.MEDIA_PROXY_FORMAT || "avif"),
    width: nonNegativeInt(__ENV.MEDIA_PROXY_WIDTH, 384),
    warmup: booleanFlag(__ENV.MEDIA_PROXY_WARMUP, mode === "cached"),
    p95Threshold: __ENV.P95_THRESHOLD || "p(95)<750",
    p99Threshold: __ENV.P99_THRESHOLD || "p(99)<1500",
  };
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

function nonNegativeInt(value: string | undefined, fallback: number): number {
  const parsed = Number.parseInt(value || "", 10);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
}
