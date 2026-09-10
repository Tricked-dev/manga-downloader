import exec from "k6/execution";
import { check } from "k6";
import { Rate, Trend } from "k6/metrics";
import {
  ResponseLike,
  SummaryData,
  TestConfig,
  apiGet,
  closedModelScenario,
  jsonResponseOk,
  loadConfig,
  makeSummary,
  targetByIteration,
  trendSummaryLine,
} from "./lib/manga.ts";

type SmokeRoute = {
  name: string;
  path: string;
  expectJson: boolean;
};

type SetupData = {
  config: TestConfig;
  routes: SmokeRoute[];
};

const config = loadConfig();
const routeOk = new Rate("manga_k6_api_smoke_ok");
const routeResponseTime = new Trend("manga_k6_api_smoke_response_time", true);

export const options = {
  scenarios: {
    api_smoke: closedModelScenario(config, "api_smoke"),
  },
  thresholds: {
    http_req_failed: ["rate==0"],
    "http_req_duration{endpoint:api_smoke}": [
      config.p95Threshold || "p(95)<500",
      config.p99Threshold || "p(99)<1000",
    ],
    manga_k6_api_smoke_ok: ["rate==1"],
  },
  summaryTrendStats: ["avg", "min", "med", "max", "p(90)", "p(95)", "p(99)"],
  userAgent: "manga-downloader-k6/2.0",
};

export function setup(): SetupData {
  const routes: SmokeRoute[] = [
    { name: "health", path: "/v1/health", expectJson: true },
    { name: "info", path: "/v1/info", expectJson: true },
    { name: "stats", path: "/v1/stats", expectJson: true },
    { name: "library", path: "/v1/library", expectJson: true },
    { name: "library_chapters", path: "/v1/library/chapters", expectJson: true },
    { name: "library_updates", path: "/v1/library/updates", expectJson: true },
    { name: "downloads", path: "/v1/downloads", expectJson: true },
    { name: "sources", path: "/v1/sources", expectJson: true },
    { name: "settings", path: "/v1/settings", expectJson: true },
    { name: "archive_index", path: "/v1/settings/archive-index", expectJson: true },
  ];

  console.log(`api_smoke routes=${routes.map((route) => route.name).join(",")}`);
  return { config, routes };
}

export default function (data: SetupData): void {
  const route = targetByIteration(data.routes, exec.scenario.iterationInTest);
  const response = apiGet(data.config, route.path, "api_smoke", { route: route.name });
  const ok = check(response, {
    "smoke route status is 2xx": (res) => res.status >= 200 && res.status < 300,
    "smoke route content type is valid": (res) =>
      !route.expectJson || jsonResponseOk(res as ResponseLike),
  });

  routeOk.add(ok, { route: route.name });
  routeResponseTime.add(response.timings.duration, { route: route.name });
}

export function handleSummary(data: SummaryData) {
  return makeSummary("API smoke", config, data, [
    trendSummaryLine("api_smoke", data.metrics?.manga_k6_api_smoke_response_time),
  ]);
}
