import exec from "k6/execution";
import { check } from "k6";
import { Rate, Trend } from "k6/metrics";
import {
  DOWNLOADED_PAGE_ROUTE,
  DownloadedPageTarget,
  ResponseLike,
  SummaryData,
  TestConfig,
  discoverDownloadedChapters,
  downloadedPageTargets,
  imageGet,
  imageResponseOk,
  loadConfig,
  makeSummary,
  targetByIteration,
  trendSummaryLine,
} from "./lib/manga.ts";

type SetupData = {
  config: TestConfig;
  targets: DownloadedPageTarget[];
};

type BurstStage = {
  duration: string;
  target: number;
};

type BurstConfig = {
  startRps: number;
  baselineRps: number;
  burstRps: number;
  troughRps: number;
  cycles: number;
  preAllocatedVUs: number;
  maxVUs: number;
  stages: BurstStage[];
};

const config = {
  ...loadConfig(),
  skipPageCache: true,
};
const burstConfig = loadBurstConfig(config);
const pageOk = new Rate("manga_k6_bursty_uncached_page_ok");
const pageResponseTime = new Trend("manga_k6_bursty_uncached_page_response_time", true);

export const options = {
  scenarios: {
    bursty_uncached_pages: {
      executor: "ramping-arrival-rate",
      startRate: burstConfig.startRps,
      timeUnit: "1s",
      preAllocatedVUs: burstConfig.preAllocatedVUs,
      maxVUs: burstConfig.maxVUs,
      stages: burstConfig.stages,
      gracefulStop: "10s",
      tags: { workload: "bursty_uncached_pages", mode: "uncached" },
    },
  },
  thresholds: {
    http_req_failed: ["rate<0.01"],
    "http_req_duration{endpoint:bursty_uncached_page}": [config.p95Threshold, config.p99Threshold],
    manga_k6_bursty_uncached_page_ok: ["rate>0.99"],
  },
  summaryTrendStats: ["avg", "min", "med", "max", "p(90)", "p(95)", "p(99)"],
  userAgent: "manga-downloader-k6/2.0",
};

export function setup(): SetupData {
  const chapters = discoverDownloadedChapters(config);
  const targetCount =
    config.urlPool > 0
      ? config.urlPool
      : Math.max(config.requests, estimatedIterations(burstConfig));
  const targets = downloadedPageTargets(config, chapters, targetCount, "unique");
  if (targets.length === 0) {
    throw new Error("No downloaded page targets were discovered");
  }

  console.log(
    [
      `bursty_uncached_pages targets=${targets.length}`,
      `start_rps=${burstConfig.startRps}`,
      `baseline_rps=${burstConfig.baselineRps}`,
      `burst_rps=${burstConfig.burstRps}`,
      `trough_rps=${burstConfig.troughRps}`,
      `cycles=${burstConfig.cycles}`,
    ].join(" "),
  );

  return { config, targets };
}

export default function (data: SetupData): void {
  const target = targetByIteration(data.targets, exec.scenario.iterationInTest);
  const response = imageGet(data.config, target.url, "bursty_uncached_page", DOWNLOADED_PAGE_ROUTE);
  const ok = check(response, {
    "bursty uncached page status/content-type is valid": (res) =>
      imageResponseOk(res as ResponseLike),
  });

  pageOk.add(ok);
  pageResponseTime.add(response.timings.duration);
}

export function handleSummary(data: SummaryData) {
  return makeSummary("bursty uncached downloaded-page reading", config, data, [
    trendSummaryLine(
      "bursty_uncached_page",
      data.metrics?.manga_k6_bursty_uncached_page_response_time,
    ),
    `  skip_page_cache: ${config.skipPageCache}`,
    `  rps_stages: ${stageSummary(burstConfig.stages)}`,
  ]);
}

function loadBurstConfig(value: TestConfig): BurstConfig {
  const baselineRps = positiveEnv("BASELINE_RPS", 25);
  const burstRps = positiveEnv("BURST_RPS", 150);
  const troughRps = nonNegativeEnv("TROUGH_RPS", 5);
  const cycles = positiveEnv("BURST_CYCLES", 3);
  const rampDuration = stringEnv("BURST_RAMP_DURATION", "15s");
  const holdDuration = stringEnv("BURST_HOLD_DURATION", "30s");
  const restDuration = stringEnv("BURST_REST_DURATION", "20s");
  const cooldownDuration = stringEnv("BURST_COOLDOWN_DURATION", "10s");
  const preAllocatedVUs = positiveEnv(
    "PRE_ALLOCATED_VUS",
    Math.max(value.concurrency, baselineRps),
  );
  const maxVUs = positiveEnv(
    "MAX_VUS",
    Math.max(preAllocatedVUs, value.concurrency * 4, burstRps * 2),
  );
  const stages = jaggedStages({
    baselineRps,
    burstRps,
    troughRps,
    cycles,
    rampDuration,
    holdDuration,
    restDuration,
    cooldownDuration,
  });

  return {
    startRps: troughRps,
    baselineRps,
    burstRps,
    troughRps,
    cycles,
    preAllocatedVUs,
    maxVUs,
    stages,
  };
}

function jaggedStages(value: {
  baselineRps: number;
  burstRps: number;
  troughRps: number;
  cycles: number;
  rampDuration: string;
  holdDuration: string;
  restDuration: string;
  cooldownDuration: string;
}): BurstStage[] {
  const highRps = lerpRate(value.baselineRps, value.burstRps, 0.72);
  const nearPeakRps = lerpRate(value.baselineRps, value.burstRps, 0.88);
  const midRps = lerpRate(value.troughRps, value.burstRps, 0.45);
  const valleyRps = lerpRate(value.troughRps, value.baselineRps, 0.35);
  const deepValleyRps = Math.max(1, Math.floor(value.troughRps / 2));
  const lateClimbRps = lerpRate(value.baselineRps, value.burstRps, 0.8);

  const stages: BurstStage[] = [];
  for (let cycle = 0; cycle < value.cycles; cycle += 1) {
    stages.push({ duration: value.rampDuration, target: highRps });
    stages.push({ duration: value.restDuration, target: midRps });
    stages.push({ duration: value.rampDuration, target: nearPeakRps });
    stages.push({ duration: value.holdDuration, target: deepValleyRps });
    stages.push({ duration: value.rampDuration, target: value.burstRps });
    stages.push({ duration: value.holdDuration, target: value.baselineRps });
    stages.push({ duration: value.rampDuration, target: highRps });
    stages.push({ duration: value.restDuration, target: valleyRps });
    stages.push({ duration: value.holdDuration, target: lateClimbRps });
  }
  stages.push({ duration: value.cooldownDuration, target: 0 });
  return stages;
}

function lerpRate(min: number, max: number, amount: number): number {
  return Math.max(1, Math.round(min + (max - min) * amount));
}

function estimatedIterations(value: BurstConfig): number {
  let currentRate = value.startRps;
  let total = 0;
  for (const stage of value.stages) {
    const seconds = durationSeconds(stage.duration);
    total += ((currentRate + stage.target) / 2) * seconds;
    currentRate = stage.target;
  }
  return Math.ceil(total);
}

function durationSeconds(value: string): number {
  const match = String(value)
    .trim()
    .match(/^(\d+(?:\.\d+)?)(ms|s|m|h)?$/);
  if (!match) {
    return 0;
  }

  const amount = Number(match[1]);
  const unit = match[2] || "s";
  if (!Number.isFinite(amount) || amount <= 0) {
    return 0;
  }
  if (unit === "ms") {
    return amount / 1000;
  }
  if (unit === "m") {
    return amount * 60;
  }
  if (unit === "h") {
    return amount * 3600;
  }
  return amount;
}

function positiveEnv(name: string, fallback: number): number {
  const parsed = parseInt(String(__ENV[name] || ""), 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function nonNegativeEnv(name: string, fallback: number): number {
  const parsed = parseInt(String(__ENV[name] || ""), 10);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
}

function stringEnv(name: string, fallback: string): string {
  const value = String(__ENV[name] || "").trim();
  return value.length > 0 ? value : fallback;
}

function stageSummary(stages: BurstStage[]): string {
  return stages.map((stage) => `${stage.duration}@${stage.target}rps`).join(" -> ");
}
