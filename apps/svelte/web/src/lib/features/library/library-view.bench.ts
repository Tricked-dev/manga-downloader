import { bench, describe } from "vitest";

import type { DownloadItem, LibraryManga } from "$lib/types";
import { getLibraryView } from "./library-view";

const CATEGORIES = ["Action", "Comedy", "Drama", "Slice of Life", "Unread"];
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

function makeManga(index: number): LibraryManga {
  return {
    author: `Author ${index % 80}`,
    auto_download: index % 9 === 0,
    category: index % 11 === 0 ? "" : (CATEGORIES[index % CATEGORIES.length] ?? "Action"),
    comic_info: {
      age_rating: "Rating Pending",
      genre: "Action, Adventure",
      language_iso: "en",
      series: `Series ${index}`,
      summary: `Description ${index}`,
      title: `Series ${index}`,
      web: `https://example.test/manga/${index}`,
      writer: `Author ${index % 80}`,
    },
    cover_proxy_url: `/api/media/${index}.avif`,
    cover_url: `https://example.test/${index}.jpg`,
    description: `Description ${index}`,
    downloaded_chapters: index % 120,
    genres: ["Action", "Adventure"],
    id: `manga-${index}`,
    is_nsfw: false,
    language: "en",
    last_updated: "2026-05-20T00:00:00Z",
    source: "Comix",
    source_base_url: "https://example.test",
    source_id: `remote-${index}`,
    status: "ongoing",
    title: `Series ${index}`,
    total_chapters: 120,
  };
}

function makeDownload(index: number): DownloadItem {
  return {
    chapter_id: `chapter-${index}`,
    chapter_number: index,
    chapter_source_id: `remote-chapter-${index}`,
    chapter_title: `Chapter ${index}`,
    error: null,
    id: `download-${index}`,
    manga_id: `manga-${index % 2_000}`,
    manga_source: "Comix",
    manga_source_id: `remote-manga-${index % 2_000}`,
    manga_title: `Series ${index % 2_000}`,
    progress: index % 100,
    status: STATUSES[index % STATUSES.length] ?? "queued",
  };
}

const library = Array.from({ length: 2_000 }, (_, index) => makeManga(index));
const downloads = Array.from({ length: 8_000 }, (_, index) => makeDownload(index));

describe("library view hot paths", () => {
  bench("groups downloads and filters library by category", () => {
    const view = getLibraryView(library, downloads, "Action");
    if (view.filteredLibrary.length === 0) {
      throw new Error("expected filtered library items");
    }
  });
});
