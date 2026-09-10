import type { DownloadItem, LibraryChapter } from "$lib/types";

export interface ChapterTableRow {
  chapter: LibraryChapter;
  download: DownloadItem | undefined;
}

export interface ChapterActionSelection {
  completedDownloads: DownloadItem[];
  downloadableChapters: LibraryChapter[];
  downloads: DownloadItem[];
  readableChapters: LibraryChapter[];
  rows: ChapterTableRow[];
  selectedCount: number;
  unreadableChapters: LibraryChapter[];
}

export interface ChapterActionAvailability {
  clearDisabled: boolean;
  deleteDisabled: boolean;
  downloadDisabled: boolean;
  markReadDisabled: boolean;
  markUnreadDisabled: boolean;
  reencodeDisabled: boolean;
}

export function getMangaDownloads(
  downloads: readonly DownloadItem[],
  libraryId: string,
): DownloadItem[] {
  return downloads.filter((download) => download.manga_id === libraryId);
}

export function getDownloadsByChapterSourceId(
  downloads: readonly DownloadItem[],
  libraryId: string,
): Map<string, DownloadItem> {
  const chapterDownloads = new Map<string, DownloadItem>();

  for (const download of downloads) {
    if (download.manga_id !== libraryId || !download.chapter_source_id) {
      continue;
    }

    chapterDownloads.set(download.chapter_source_id, download);
  }

  return chapterDownloads;
}

export function getChapterRows(
  chapters: readonly LibraryChapter[],
  downloadsBySourceId: ReadonlyMap<string, DownloadItem>,
): ChapterTableRow[] {
  return chapters.map((chapter) => ({
    chapter,
    download: downloadsBySourceId.get(chapter.source_id),
  }));
}

export function getChapterActionSelection(
  rows: readonly ChapterTableRow[],
): ChapterActionSelection {
  const downloads: DownloadItem[] = [];
  const completedDownloads: DownloadItem[] = [];
  const downloadableChapters: LibraryChapter[] = [];
  const readableChapters: LibraryChapter[] = [];
  const unreadableChapters: LibraryChapter[] = [];

  for (const row of rows) {
    readableChapters.push(row.chapter);

    if (hasReadProgress(row.chapter)) {
      unreadableChapters.push(row.chapter);
    }

    if (!row.chapter.downloaded && !row.download) {
      downloadableChapters.push(row.chapter);
    }

    if (!row.download) {
      continue;
    }

    downloads.push(row.download);
    if (row.download.status === "completed") {
      completedDownloads.push(row.download);
    }
  }

  return {
    completedDownloads,
    downloadableChapters,
    downloads,
    readableChapters,
    rows: [...rows],
    selectedCount: rows.length,
    unreadableChapters,
  };
}

export function getChapterActionAvailability(
  selection: ChapterActionSelection,
  actionLoading: boolean,
): ChapterActionAvailability {
  return {
    clearDisabled: selection.selectedCount === 0 || actionLoading,
    deleteDisabled: selection.downloads.length === 0 || actionLoading,
    downloadDisabled: selection.downloadableChapters.length === 0 || actionLoading,
    markReadDisabled: selection.selectedCount === 0 || actionLoading,
    markUnreadDisabled:
      selection.selectedCount === 0 || actionLoading || selection.unreadableChapters.length === 0,
    reencodeDisabled: selection.completedDownloads.length === 0 || actionLoading,
  };
}

export function orderChaptersForChronologicalDownload(
  chapters: readonly LibraryChapter[],
): LibraryChapter[] {
  return [...chapters].sort(compareChaptersChronologically);
}

function compareChaptersChronologically(left: LibraryChapter, right: LibraryChapter): number {
  const chapterNumberOrder =
    normalizedChapterNumber(left.chapter_number) - normalizedChapterNumber(right.chapter_number);
  if (chapterNumberOrder !== 0) {
    return chapterNumberOrder;
  }

  const uploadedOrder =
    normalizedTimestamp(left.date_uploaded) - normalizedTimestamp(right.date_uploaded);
  if (uploadedOrder !== 0) {
    return uploadedOrder;
  }

  const fetchedOrder = normalizedTimestamp(left.fetched_at) - normalizedTimestamp(right.fetched_at);
  if (fetchedOrder !== 0) {
    return fetchedOrder;
  }

  return left.id.localeCompare(right.id);
}

function normalizedChapterNumber(value: number): number {
  return Number.isFinite(value) ? value : Number.POSITIVE_INFINITY;
}

function normalizedTimestamp(value: string): number {
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function hasReadProgress(chapter: LibraryChapter): boolean {
  return chapter.read_completed || chapter.pages_read > 0;
}
