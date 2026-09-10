import { browser } from "$app/environment";
import {
  createGetDownloadedChapterPages,
  createGetPageList,
  createListSources,
  updateChapterReadProgress,
} from "@manga-server/api-client/generated";
import { isApiErrorCode, requestRaw } from "@manga-server/api-client/core";
import { apiResourceUrl, imageProxyUrl } from "@manga-server/api-client/urls";
import { SvelteMap } from "svelte/reactivity";
import { QUERY_CACHE_TIMES } from "$lib/query-client";
import { selectItems } from "$lib/query-selectors";
import { getErrorMessage, normalizeAbsoluteUrl } from "$lib/utils";
import type { Source } from "$lib/types";
import {
  clampPageIndex,
  getScrollRenderRange,
  isDownloadedPageRoute,
  nextReadProgressReport,
  requiresBaseUrl,
  withSkipPageCache,
  type ReaderMode,
  type ReadProgressState,
} from "./reader-session-policy";

const SCROLL_IMAGE_OVERSCAN = 4;
const SCROLL_PAGE_PLACEHOLDER_HEIGHT = "min(145vw, 72rem)";
const EMPTY_PAGES: string[] = [];

export interface ReaderSessionOptions {
  getChapterSourceId: () => string;
  getLibraryMangaId?: () => string;
  getLibraryChapterId: () => string;
  getSourceName: () => string;
  canRecordReadProgress?: () => boolean;
  translate: (key: string, vars?: Record<string, unknown>) => string;
}

export function createReaderSession({
  getChapterSourceId,
  getLibraryMangaId = () => "",
  getLibraryChapterId,
  getSourceName,
  canRecordReadProgress = () => true,
  translate,
}: ReaderSessionOptions) {
  const sourceName = $derived(getSourceName());
  const chapterSourceId = $derived(getChapterSourceId());
  const libraryMangaId = $derived(getLibraryMangaId());
  const libraryChapterId = $derived(getLibraryChapterId());
  const pageListSourceName = $derived(encodeURIComponent(sourceName));
  const pageListChapterSourceId = $derived(encodeURIComponent(chapterSourceId));
  const sourcePagesQuery = createGetPageList(
    () => pageListSourceName,
    () => pageListChapterSourceId,
    undefined,
    () => ({
      query: {
        enabled:
          browser &&
          libraryChapterId.length === 0 &&
          sourceName.length > 0 &&
          chapterSourceId.length > 0,
        select: selectItems<string>,
        staleTime: QUERY_CACHE_TIMES.stable,
      },
    }),
  );
  const downloadedPagesQuery = createGetDownloadedChapterPages(
    () => libraryChapterId,
    () => publicDownloadedPageParams(),
    () => ({
      query: {
        enabled: browser && libraryChapterId.length > 0,
        select: selectItems<string>,
        staleTime: QUERY_CACHE_TIMES.stable,
      },
    }),
  );
  const useDownloadedPages = $derived(libraryChapterId.length > 0);
  const sourcesQuery = createListSources(() => ({
    query: {
      enabled: browser && sourceName.length > 0 && !useDownloadedPages,
      select: selectItems<Source>,
      staleTime: QUERY_CACHE_TIMES.stable,
    },
  }));

  const pages: string[] = $derived(
    (useDownloadedPages ? downloadedPagesQuery.data : sourcePagesQuery.data) ?? EMPTY_PAGES,
  );
  const sourceBaseUrl = $derived(
    sourcesQuery.data?.find((source: Source) => source.name === sourceName)?.base_url ?? null,
  );
  const pageUrlsReady = $derived(
    useDownloadedPages ||
      pages.every((pageUrl) => !requiresBaseUrl(pageUrl)) ||
      sourceBaseUrl !== null,
  );
  const loading = $derived(
    ((useDownloadedPages ? downloadedPagesQuery.isPending : sourcePagesQuery.isPending) &&
      pages.length === 0) ||
      (pages.length > 0 && !pageUrlsReady),
  );
  const pageLoadError = $derived(
    (useDownloadedPages ? downloadedPagesQuery.error : sourcePagesQuery.error)
      ? getErrorMessage(
          useDownloadedPages ? downloadedPagesQuery.error : sourcePagesQuery.error,
          translate("app.reader.failedToLoadPages"),
        )
      : !useDownloadedPages && sourcesQuery.error
        ? getErrorMessage(sourcesQuery.error, translate("app.reader.failedToLoadSource"))
        : "",
  );
  let currentPage = $state(0);
  let mode: ReaderMode = $state("scroll");
  let imageVersion = $state<"best" | "original">("best");
  let imageLoadFailed = $state(false);
  let imageLoadConflict = $state(false);
  let firstFailedUrl = $state("");
  let activeScrollPage = $state(0);
  let scrollFrame = 0;
  let readProgressState: ReadProgressState = { chapterId: "", highestReportedPage: 0 };
  const scrollPageHeights = new SvelteMap<number, number>();

  const currentPageIndex = $derived(clampPageIndex(currentPage, pages.length));
  const activeScrollPageIndex = $derived(clampPageIndex(activeScrollPage, pages.length));
  const pageCountLabel = $derived(getPageCountLabel());
  const scrollRenderRange = $derived(
    getScrollRenderRange(activeScrollPageIndex, pages.length, SCROLL_IMAGE_OVERSCAN),
  );

  $effect(() => {
    if (!browser || !libraryChapterId || pages.length === 0 || !canRecordReadProgress()) {
      return;
    }

    const pageIndex = mode === "page" ? currentPageIndex : activeScrollPageIndex;
    void reportReadProgress(libraryChapterId, pageIndex);
  });

  function getPageCountLabel() {
    if (loading) {
      return translate("app.reader.loadingPages");
    }

    if (pageLoadError) {
      return translate("app.reader.pagesUnavailable");
    }

    if (pages.length === 0) {
      return translate("app.reader.noPages");
    }

    return mode === "page"
      ? translate("app.reader.pageCount", { page: currentPageIndex + 1, total: pages.length })
      : translate("app.reader.pages", { count: pages.length });
  }

  function pageImageUrl(pageUrl: string): string {
    if (browser && isDownloadedPageRoute(pageUrl, window.location.origin)) {
      return apiResourceUrl(downloadedPageApiPath(pageUrl));
    }

    return imageProxyUrl(pageUrl, sourceBaseUrl);
  }

  function handleImageError(pageUrl: string) {
    if (!imageLoadFailed) {
      firstFailedUrl = normalizeAbsoluteUrl(pageUrl, sourceBaseUrl);
    }
    imageLoadFailed = true;
    void detectDownloadedPageConflict(pageUrl);
  }

  function retryProxyImages() {
    imageLoadFailed = false;
    imageLoadConflict = false;
  }

  function goBack() {
    history.back();
  }

  function setScrollMode() {
    setReaderMode("scroll");
  }

  function setPageMode() {
    setReaderMode("page");
  }

  function goToPreviousPage() {
    currentPage = clampPageIndex(currentPageIndex - 1, pages.length);
  }

  function goToNextPage() {
    currentPage = clampPageIndex(currentPageIndex + 1, pages.length);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (mode !== "page") {
      return;
    }

    if (event.key === "ArrowRight" || event.key === " ") {
      currentPage = clampPageIndex(currentPageIndex + 1, pages.length);
    } else if (event.key === "ArrowLeft") {
      currentPage = clampPageIndex(currentPageIndex - 1, pages.length);
    }
  }

  async function reportReadProgress(currentLibraryChapterId: string, pageIndex: number) {
    const { nextState, report } = nextReadProgressReport(
      readProgressState,
      currentLibraryChapterId,
      pageIndex,
      pages.length,
    );
    readProgressState = nextState;

    if (!report) {
      return;
    }

    try {
      await updateChapterReadProgress(currentLibraryChapterId, report);
    } catch {
      // Read Progress is opportunistic; reader navigation should not fail if it cannot be recorded.
    }
  }

  function scrollPageHeight(index: number): string {
    const height = scrollPageHeights.get(index);
    return height ? `${Math.round(height)}px` : SCROLL_PAGE_PLACEHOLDER_HEIGHT;
  }

  function shouldRenderScrollPage(index: number): boolean {
    return index >= scrollRenderRange.start && index <= scrollRenderRange.end;
  }

  function recordScrollPageHeight(index: number, event: Event) {
    const height = (event.currentTarget as HTMLImageElement).getBoundingClientRect().height;
    if (height > 0) {
      scrollPageHeights.set(index, height);
    }
  }

  function updateActiveScrollPage() {
    scrollFrame = 0;

    if (!browser || mode !== "scroll") {
      return;
    }

    const pageElements = document.querySelectorAll<HTMLElement>("[data-reader-page-index]");
    const anchorY = window.innerHeight * 0.35;
    let nextPage = activeScrollPage;
    let closestDistance = Number.POSITIVE_INFINITY;

    for (const element of pageElements) {
      const rect = element.getBoundingClientRect();
      if (rect.bottom < 0) {
        continue;
      }

      const distance = Math.abs(rect.top - anchorY);
      if (distance < closestDistance) {
        closestDistance = distance;
        nextPage = Number(element.dataset.readerPageIndex ?? 0);
      }

      if (rect.top > anchorY && closestDistance < Number.POSITIVE_INFINITY) {
        break;
      }
    }

    activeScrollPage = nextPage;
  }

  function scheduleActiveScrollPageUpdate() {
    if (!browser || scrollFrame !== 0) {
      return;
    }

    scrollFrame = requestAnimationFrame(updateActiveScrollPage);
  }

  function scrollToPage(index: number) {
    if (!browser) {
      return;
    }

    requestAnimationFrame(() => {
      document
        .querySelector<HTMLElement>(`[data-reader-page-index="${index}"]`)
        ?.scrollIntoView({ block: "start" });
    });
  }

  function setReaderMode(nextMode: ReaderMode) {
    if (nextMode === mode) {
      return;
    }

    const targetPage = nextMode === "page" ? activeScrollPageIndex : currentPageIndex;
    if (nextMode === "page") {
      currentPage = targetPage;
    } else {
      activeScrollPage = targetPage;
    }

    mode = nextMode;
    if (nextMode === "scroll") {
      scrollToPage(targetPage);
      scheduleActiveScrollPageUpdate();
    }
  }

  async function retryLoadPages() {
    if (useDownloadedPages) {
      await downloadedPagesQuery.refetch();
      return;
    }

    await Promise.all([sourcePagesQuery.refetch(), sourcesQuery.refetch()]);
  }

  async function detectDownloadedPageConflict(pageUrl: string) {
    if (!browser || !isDownloadedPageRoute(pageUrl, window.location.origin)) {
      return;
    }

    try {
      await requestRaw(withSkipPageCache(downloadedPageApiPath(pageUrl), window.location.origin), {
        headers: {
          Accept: "image/*,*/*;q=0.8",
        },
      });
    } catch (error) {
      if (isApiErrorCode(error, "downloaded_page_conflict")) {
        imageLoadConflict = true;
      }
    }
  }

  function publicDownloadedPageParams(): { libraryId?: string; skip_page_cache?: boolean } {
    return libraryMangaId ? { libraryId: libraryMangaId } : {};
  }

  function downloadedPageApiPath(pageUrl: string): string {
    const parsed = new URL(pageUrl, window.location.origin);
    if (libraryMangaId) {
      parsed.searchParams.set("libraryId", libraryMangaId);
    }
    if (imageVersion === "original") parsed.searchParams.set("variant", "original");
    else parsed.searchParams.delete("variant");
    return `${parsed.pathname}${parsed.search}`;
  }

  return {
    get imageVersion() { return imageVersion; },
    get downloaded() { return useDownloadedPages; },
    setImageVersion(value: "best" | "original") { imageVersion = value; retryProxyImages(); },
    get currentPageIndex() {
      return currentPageIndex;
    },
    get firstFailedUrl() {
      return firstFailedUrl;
    },
    goBack,
    goToNextPage,
    goToPreviousPage,
    handleImageError,
    handleKeydown,
    get imageLoadConflict() {
      return imageLoadConflict;
    },
    get imageLoadFailed() {
      return imageLoadFailed;
    },
    get loading() {
      return loading;
    },
    get mode() {
      return mode;
    },
    get pageCountLabel() {
      return pageCountLabel;
    },
    pageImageUrl,
    get pageLoadError() {
      return pageLoadError;
    },
    get pages() {
      return pages;
    },
    recordScrollPageHeight,
    retryLoadPages,
    retryProxyImages,
    scheduleActiveScrollPageUpdate,
    scrollPageHeight,
    setPageMode,
    setScrollMode,
    shouldRenderScrollPage,
    get sourceBaseUrl() {
      return sourceBaseUrl;
    },
  };
}
