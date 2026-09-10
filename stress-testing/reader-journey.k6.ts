import exec from "k6/execution";
import { check, sleep } from "k6";
import { Counter, Rate, Trend } from "k6/metrics";
import {
  DOWNLOADED_PAGE_LIST_ROUTE,
  DOWNLOADED_PAGE_ROUTE,
  DownloadedChapter,
  DownloadedPageTarget,
  ResponseLike,
  SummaryData,
  TestConfig,
  apiGet,
  closedModelScenario,
  discoverDownloadedChapters,
  downloadedPageListPath,
  downloadedPageTargets,
  imageGet,
  imageResponseOk,
  jsonResponseOk,
  loadConfig,
  makeSummary,
  randomThinkSeconds,
  targetByIteration,
  trendSummaryLine,
} from "./lib/manga.ts";

type SetupData = {
  config: TestConfig;
  chapters: DownloadedChapter[];
  pages: DownloadedPageTarget[];
};

const config = loadConfig();
const journeyOk = new Rate("manga_k6_reader_journey_ok");
const apiResponseTime = new Trend("manga_k6_reader_api_response_time", true);
const pageResponseTime = new Trend("manga_k6_reader_page_response_time", true);
const pagesRead = new Counter("manga_k6_reader_pages_total");

export const options = {
  scenarios: {
    reader_journey: closedModelScenario(config, "reader_journey"),
  },
  thresholds: {
    http_req_failed: ["rate<0.01"],
    "http_req_duration{endpoint:reader_page}": [config.p95Threshold, config.p99Threshold],
    manga_k6_reader_journey_ok: ["rate>0.99"],
  },
  summaryTrendStats: ["avg", "min", "med", "max", "p(90)", "p(95)", "p(99)"],
  userAgent: "manga-downloader-k6/2.0",
};

export function setup(): SetupData {
  const chapters = discoverDownloadedChapters(config);
  const targetCount = config.urlPool > 0 ? config.urlPool : Math.max(config.requests, 100);
  const pages = downloadedPageTargets(config, chapters, targetCount, "reader");
  if (chapters.length === 0 || pages.length === 0) {
    throw new Error("No downloaded reader targets were discovered");
  }

  console.log(
    `reader_journey chapters=${chapters.length} pages=${pages.length} pages_per_journey=${config.pagesPerJourney}`,
  );
  return { config, chapters, pages };
}

export default function (data: SetupData): void {
  let ok = true;

  ok = requestJson(data.config, "/v1/library", "reader_api", "library") && ok;
  ok = requestJson(data.config, "/v1/library/chapters", "reader_api", "library_chapters") && ok;

  const chapter = targetByIteration(data.chapters, exec.scenario.iterationInTest);
  ok =
    requestJson(
      data.config,
      downloadedPageListPath(chapter.id, data.config.skipPageCache),
      "reader_page_list",
      DOWNLOADED_PAGE_LIST_ROUTE,
    ) && ok;

  for (let offset = 0; offset < data.config.pagesPerJourney; offset += 1) {
    const page = targetByIteration(
      data.pages,
      exec.scenario.iterationInTest * data.config.pagesPerJourney + offset,
    );
    const response = imageGet(data.config, page.url, "reader_page", DOWNLOADED_PAGE_ROUTE);
    const pageOk = check(response, {
      "reader page status/content-type is valid": (res) => imageResponseOk(res as ResponseLike),
    });
    ok = pageOk && ok;
    pagesRead.add(1);
    pageResponseTime.add(response.timings.duration);
  }

  journeyOk.add(ok);
  const thinkTime = randomThinkSeconds(data.config);
  if (thinkTime > 0) {
    sleep(thinkTime);
  }
}

export function handleSummary(data: SummaryData) {
  return makeSummary("reader journey", config, data, [
    trendSummaryLine("reader_api", data.metrics?.manga_k6_reader_api_response_time),
    trendSummaryLine("reader_page", data.metrics?.manga_k6_reader_page_response_time),
    `  pages_per_journey: ${config.pagesPerJourney}`,
    `  skip_page_cache: ${config.skipPageCache}`,
  ]);
}

function requestJson(config: TestConfig, path: string, endpoint: string, name: string): boolean {
  const response = apiGet(config, path, endpoint, { name });
  const ok = check(response, {
    "reader API status/content-type is valid": (res) => jsonResponseOk(res as ResponseLike),
  });
  apiResponseTime.add(response.timings.duration, { endpoint });
  return ok;
}
