export type ReaderMode = "scroll" | "page";

export interface ScrollRenderRange {
  end: number;
  start: number;
}

export interface ReadProgressReport {
  completed: boolean;
  page: number;
}

export interface ReadProgressState {
  chapterId: string;
  highestReportedPage: number;
}

export function clampPageIndex(pageIndex: number, pageCount: number): number {
  return Math.min(Math.max(pageIndex, 0), Math.max(pageCount - 1, 0));
}

export function getScrollRenderRange(
  activePageIndex: number,
  pageCount: number,
  overscan: number,
): ScrollRenderRange {
  return {
    end: Math.min(pageCount - 1, activePageIndex + overscan),
    start: Math.max(0, activePageIndex - overscan),
  };
}

export function requiresBaseUrl(pageUrl: string): boolean {
  const trimmedUrl = pageUrl.trim();
  return trimmedUrl.startsWith("/") && !trimmedUrl.startsWith("//");
}

export function nextReadProgressReport(
  state: ReadProgressState,
  libraryChapterId: string,
  pageIndex: number,
  pageCount: number,
): {
  nextState: ReadProgressState;
  report: ReadProgressReport | null;
} {
  if (!libraryChapterId || pageCount === 0) {
    return { nextState: state, report: null };
  }

  const currentState =
    state.chapterId === libraryChapterId
      ? state
      : { chapterId: libraryChapterId, highestReportedPage: 0 };
  const page = Math.min(Math.max(pageIndex + 1, 1), pageCount);
  const completed = page >= pageCount;

  if (!completed && page <= currentState.highestReportedPage) {
    return { nextState: currentState, report: null };
  }

  return {
    nextState: {
      chapterId: libraryChapterId,
      highestReportedPage: Math.max(currentState.highestReportedPage, page),
    },
    report: { completed, page },
  };
}

export function isDownloadedPageRoute(pageUrl: string, origin: string): boolean {
  try {
    const parsed = new URL(pageUrl, origin);
    return /^\/v1\/library\/chapters\/[^/]+\/pages\/\d+$/.test(parsed.pathname);
  } catch {
    return false;
  }
}

export function withSkipPageCache(pageUrl: string, origin: string): string {
  const parsed = new URL(pageUrl, origin);
  parsed.searchParams.set("skip_page_cache", "true");
  return `${parsed.pathname}${parsed.search}`;
}
