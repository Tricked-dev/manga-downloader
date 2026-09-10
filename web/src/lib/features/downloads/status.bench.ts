import { bench, describe } from "vitest";

import type { DownloadItem } from "$lib/types";
import { getDownloadStatusCounts } from "./status";

const STATUSES = [
  "queued",
  "fetch",
  "downloading",
  "conversion",
  "archive",
  "completed",
  "error",
  "cancelled",
];

function makeDownload(index: number): DownloadItem {
  return {
    chapter_id: `chapter-${index}`,
    chapter_number: index,
    chapter_source_id: `remote-chapter-${index}`,
    chapter_title: `Chapter ${index}`,
    error: index % 7 === 0 ? "network timeout" : null,
    id: `download-${index}`,
    manga_id: `manga-${index % 250}`,
    manga_source: "Comix",
    manga_source_id: `remote-manga-${index % 250}`,
    manga_title: `Series ${index % 250}`,
    progress: index % 100,
    status: STATUSES[index % STATUSES.length] ?? "queued",
  };
}

const downloads = Array.from({ length: 20_000 }, (_, index) => makeDownload(index));

describe("download status hot paths", () => {
  bench("counts status buckets for a large queue", () => {
    const counts = getDownloadStatusCounts(downloads);
    if (counts.active === 0) {
      throw new Error("expected active downloads");
    }
  });
});
