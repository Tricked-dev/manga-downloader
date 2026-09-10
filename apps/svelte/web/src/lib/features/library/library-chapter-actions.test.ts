import { describe, expect, it } from "vitest";

import type { DownloadItem, LibraryChapter } from "$lib/types";
import {
  getChapterActionAvailability,
  getChapterActionSelection,
  getChapterRows,
  getDownloadsByChapterSourceId,
  getMangaDownloads,
  orderChaptersForChronologicalDownload,
} from "./library-chapter-actions";

function chapter(overrides: Partial<LibraryChapter> = {}): LibraryChapter {
  return {
    chapter_number: 1,
    date_uploaded: "",
    downloaded: false,
    fetched_at: "",
    id: "chapter-1",
    is_new: false,
    last_read_at: null,
    manga_id: "series-1",
    pages_read: 0,
    read_completed: false,
    source_id: "remote-chapter-1",
    title: "",
    ...overrides,
  };
}

function download(overrides: Partial<DownloadItem> = {}): DownloadItem {
  return {
    chapter_id: "chapter-1",
    chapter_number: 1,
    chapter_source_id: "remote-chapter-1",
    chapter_title: "Chapter 1",
    error: null,
    id: "download-1",
    manga_id: "series-1",
    manga_source: "demo",
    manga_source_id: "remote-series-1",
    manga_title: "Series",
    progress: 0,
    status: "queued",
    ...overrides,
  };
}

describe("Local Library Chapter action policy", () => {
  it("maps Downloads to Local Library Chapters by source id", () => {
    const matchingDownload = download();
    const unrelatedDownload = download({
      chapter_source_id: "remote-chapter-2",
      id: "download-2",
      manga_id: "series-2",
    });

    const downloadsBySourceId = getDownloadsByChapterSourceId(
      [matchingDownload, unrelatedDownload],
      "series-1",
    );

    expect(getMangaDownloads([matchingDownload, unrelatedDownload], "series-1")).toEqual([
      matchingDownload,
    ]);
    expect(getChapterRows([chapter()], downloadsBySourceId)).toEqual([
      { chapter: chapter(), download: matchingDownload },
    ]);
  });

  it("derives selected chapter action groups and disabled state", () => {
    const queued = { chapter: chapter(), download: download({ status: "queued" }) };
    const completed = {
      chapter: chapter({ id: "chapter-2", source_id: "remote-chapter-2" }),
      download: download({
        chapter_id: "chapter-2",
        chapter_source_id: "remote-chapter-2",
        id: "download-2",
        status: "completed",
      }),
    };
    const unreadable = {
      chapter: chapter({
        id: "chapter-3",
        pages_read: 2,
        source_id: "remote-chapter-3",
      }),
      download: undefined,
    };

    const selection = getChapterActionSelection([queued, completed, unreadable]);
    expect(selection.downloads).toHaveLength(2);
    expect(selection.completedDownloads).toHaveLength(1);
    expect(selection.downloadableChapters).toEqual([unreadable.chapter]);
    expect(selection.unreadableChapters).toEqual([unreadable.chapter]);

    expect(getChapterActionAvailability(selection, false)).toEqual({
      clearDisabled: false,
      deleteDisabled: false,
      downloadDisabled: false,
      markReadDisabled: false,
      markUnreadDisabled: false,
      reencodeDisabled: false,
    });

    expect(getChapterActionAvailability(selection, true)).toEqual({
      clearDisabled: true,
      deleteDisabled: true,
      downloadDisabled: true,
      markReadDisabled: true,
      markUnreadDisabled: true,
      reencodeDisabled: true,
    });
  });

  it("disables actions when a selection lacks eligible Local Library Chapters or Downloads", () => {
    const selected = getChapterActionSelection([
      { chapter: chapter({ downloaded: true }), download: undefined },
    ]);

    expect(getChapterActionAvailability(selected, false)).toMatchObject({
      deleteDisabled: true,
      downloadDisabled: true,
      markReadDisabled: false,
      markUnreadDisabled: true,
      reencodeDisabled: true,
    });
  });

  it("orders bulk Download work chronologically", () => {
    const chapters = [
      chapter({ chapter_number: 5, id: "chapter-5", source_id: "remote-chapter-5" }),
      chapter({
        chapter_number: 2,
        date_uploaded: "2024-02-01T00:00:00Z",
        id: "chapter-2-newer",
        source_id: "remote-chapter-2-newer",
      }),
      chapter({ chapter_number: 1.5, id: "chapter-1.5", source_id: "remote-chapter-1.5" }),
      chapter({
        chapter_number: 2,
        date_uploaded: "2024-01-01T00:00:00Z",
        id: "chapter-2-older",
        source_id: "remote-chapter-2-older",
      }),
    ];

    expect(orderChaptersForChronologicalDownload(chapters).map((item) => item.id)).toEqual([
      "chapter-1.5",
      "chapter-2-older",
      "chapter-2-newer",
      "chapter-5",
    ]);
  });
});
