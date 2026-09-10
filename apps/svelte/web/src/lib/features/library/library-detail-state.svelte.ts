import { browser } from "$app/environment";
import { goto } from "$app/navigation";
import { resolve } from "$app/paths";
import {
  createDeleteDownload,
  createEnqueueDownloads,
  createClearChapterReadProgress,
  createGetDownloads,
  createGetLibraryManga,
  createGetLibraryMangaChapters,
  createGetSettings,
  createReencodeDownload,
  createRefreshLibraryMangaChapters,
  createRefreshLibraryMangaMetadata,
  createRemoveFromLibrary,
  createUpdateChapterReadProgress,
  createUpdateLibraryMangaCategory,
  getGetLibraryMangaChaptersQueryKey,
  getGetLibraryMangaQueryKey,
  getGetSettingsQueryKey,
} from "@manga-server/api-client/generated";
import {
  createSortedRowModel,
  createTable,
  rowSelectionFeature,
  rowSortingFeature,
  sortFns,
  tableFeatures,
  type ColumnDef,
} from "@tanstack/svelte-table";
import {
  ACTIVE_DOWNLOAD_STATUSES,
  getTranslatedDownloadDisplayLabel,
} from "$lib/features/downloads/status";
import {
  getChapterActionAvailability,
  getChapterActionSelection,
  getChapterRows,
  getDownloadsByChapterSourceId,
  getMangaDownloads,
  type ChapterTableRow,
} from "$lib/features/library/library-chapter-actions";
import {
  deleteLocalLibraryDownloads,
  invalidateLocalLibraryDownloads,
  invalidateLocalLibraryState,
  markLocalLibraryChaptersRead,
  markLocalLibraryChaptersUnread,
  queueLocalLibraryChaptersForDownload,
  reencodeLocalLibraryDownloads,
  refreshLocalLibraryChapters,
  refreshLocalLibraryComicInfo,
  removeLocalLibraryManga,
  saveLocalLibraryCategory,
} from "$lib/features/library/library-detail-commands";
import { getErrorMessage, parseCsvSetting } from "$lib/utils";
import { useQueryClient, type DehydratedState } from "@tanstack/svelte-query";
import { page } from "$app/state";
import { selectItems, selectSettings } from "$lib/query-selectors";
import { QUERY_CACHE_TIMES, visibleRefetchInterval } from "$lib/query-client";
import { dehydratedQueryOptions } from "$lib/query-hydration";
import type { DownloadItem, LibraryChapter, LibraryManga } from "$lib/types";

export function createLibraryDetailState(
  getTranslate: () => (key: string, vars?: Record<string, unknown>) => string,
  options: { dehydratedState?: DehydratedState; isPublicView?: () => boolean } = {},
) {
  const EMPTY_CATEGORIES: string[] = [];
  const EMPTY_CHAPTERS: LibraryChapter[] = [];
  const EMPTY_DOWNLOADS: DownloadItem[] = [];

  const queryClient = useQueryClient();
  const publicView = $derived(options.isPublicView?.() ?? false);

  function translate(key: string, vars?: Record<string, unknown>) {
    return getTranslate()(key, vars);
  }

  function errorMessage(error: unknown) {
    return getErrorMessage(error, translate("app.errors.genericShort"));
  }

  const libraryId = $derived(page.params.seriesId ?? "");
  let selectedCategory = $state("");
  let savingCategory = $state(false);
  let categorySaveMessage = $state("");
  let chapterActionMessage = $state("");
  let chapterActionMessageIsError = $state(false);
  let bulkDownloadLoading = $state(false);
  let refreshMessage = $state("");
  let refreshMessageIsError = $state(false);
  let comicInfoRefreshMessage = $state("");
  let comicInfoRefreshMessageIsError = $state(false);
  let removeMessage = $state("");
  let removeMessageIsError = $state(false);
  let selectedDeleteLoading = $state(false);
  let selectedReencodeLoading = $state(false);
  let selectedReadLoading = $state(false);
  let selectedUnreadLoading = $state(false);
  let downloadsQuery: ReturnType<typeof createGetDownloads<DownloadItem[]>> | undefined;
  const settingsQuery = createGetSettings(() => ({
    query: {
      enabled: browser && !publicView,
      ...dehydratedQueryOptions(options.dehydratedState, getGetSettingsQueryKey()),
      select: selectSettings,
      staleTime: QUERY_CACHE_TIMES.stable,
    },
  }));

  const mangaQuery = createGetLibraryManga(
    () => libraryId,
    () => ({
      query: {
        enabled: browser && libraryId.length > 0,
        ...dehydratedQueryOptions(options.dehydratedState, getGetLibraryMangaQueryKey(libraryId)),
        staleTime: QUERY_CACHE_TIMES.stable,
      },
    }),
  );

  downloadsQuery = createGetDownloads<DownloadItem[]>(
    () => ({ manga_id: libraryId }),
    () => ({
      query: {
        enabled: browser && !publicView && libraryId.length > 0,
        refetchInterval: visibleRefetchInterval(4000, shouldPollMangaDownloads),
        select: selectItems<DownloadItem>,
        staleTime: QUERY_CACHE_TIMES.active,
      },
    }),
  );

  const chaptersQuery = createGetLibraryMangaChapters(
    () => libraryId,
    () => ({
      query: {
        enabled: browser && libraryId.length > 0,
        ...dehydratedQueryOptions(
          options.dehydratedState,
          getGetLibraryMangaChaptersQueryKey(libraryId),
        ),
        refetchInterval: visibleRefetchInterval(10_000, shouldPollMangaDownloads),
        select: selectItems<LibraryChapter>,
        staleTime: QUERY_CACHE_TIMES.live,
      },
    }),
  );

  const downloadBulkMutation = createEnqueueDownloads();

  const removeMutation = createDeleteDownload();

  const reencodeMutation = createReencodeDownload();

  const categoryMutation = createUpdateLibraryMangaCategory();

  const refreshChaptersMutation = createRefreshLibraryMangaChapters();

  const refreshComicInfoMutation = createRefreshLibraryMangaMetadata();

  const removeFromLibraryMutation = createRemoveFromLibrary();

  const updateReadProgressMutation = createUpdateChapterReadProgress();

  const clearReadProgressMutation = createClearChapterReadProgress();

  const availableCategories = $derived(
    parseCsvSetting(settingsQuery.data?.library_categories, EMPTY_CATEGORIES),
  );

  const manga = $derived(mangaQuery.data ?? null);
  const chapters = $derived(chaptersQuery.data ?? EMPTY_CHAPTERS);
  const downloads: DownloadItem[] = $derived(downloadsQuery?.data ?? EMPTY_DOWNLOADS);
  const comicInfoRows = $derived(getComicInfoRows(manga));
  function shouldPollMangaDownloads() {
    return getMangaDownloads(downloadsQuery?.data ?? EMPTY_DOWNLOADS, libraryId).some((download) =>
      ACTIVE_DOWNLOAD_STATUSES.has(download.status),
    );
  }

  function getComicInfoRows(currentManga: LibraryManga | null): [string, string][] {
    if (!currentManga) {
      return [];
    }

    const comicInfoSeries = currentManga.comic_info?.series?.trim();
    const comicInfoWriter = currentManga.comic_info?.writer?.trim();
    const heroAuthor = currentManga.author?.trim();
    const authorMetadata = heroAuthor || comicInfoWriter;
    const rows: Array<[string, string | undefined | null]> = [
      [
        translate("app.libraryDetail.series"),
        comicInfoSeries && comicInfoSeries !== currentManga.title ? comicInfoSeries : undefined,
      ],
      [translate("app.libraryDetail.author"), authorMetadata],
      [
        translate("app.libraryDetail.language"),
        currentManga.comic_info?.language_iso ?? currentManga.language,
      ],
      [translate("app.libraryDetail.ageRating"), currentManga.comic_info?.age_rating],
      [translate("app.libraryDetail.genre"), currentManga.comic_info?.genre],
    ];

    return rows.filter((row): row is [string, string] => Boolean(row[1]?.trim()));
  }
  const pageLoading = $derived(!manga && mangaQuery.isPending);
  const pageError = $derived(
    mangaQuery.error
      ? getErrorMessage(mangaQuery.error, translate("app.libraryDetail.failedLoadLibraryManga"))
      : "",
  );
  const chaptersLoading = $derived(chaptersQuery.isPending && chapters.length === 0);
  const chaptersError = $derived(
    chaptersQuery.error
      ? getErrorMessage(chaptersQuery.error, translate("app.manga.failedToLoadChapters"))
      : downloadsQuery.error
        ? getErrorMessage(
            downloadsQuery.error,
            translate("app.libraryDetail.failedLoadDownloadStatus"),
          )
        : "",
  );
  const initialCategory = $derived(
    manga?.category && availableCategories.includes(manga.category) ? manga.category : "",
  );
  const effectiveSelectedCategory = $derived(
    selectedCategory || initialCategory || availableCategories[0] || "",
  );
  const downloadByChapterSourceId = $derived(getDownloadsByChapterSourceId(downloads, libraryId));
  const chapterRows = $derived(getChapterRows(chapters, downloadByChapterSourceId));

  const _chapterTableFeatures = tableFeatures({
    rowSelectionFeature,
    rowSortingFeature,
  });

  const chapterColumns: ColumnDef<typeof _chapterTableFeatures, ChapterTableRow>[] = [
    {
      id: "select",
      enableSorting: false,
    },
    {
      id: "chapter",
      accessorFn: (row: ChapterTableRow) => row.chapter.chapter_number,
      header: "app.downloads.chapter",
    },
    {
      id: "uploaded",
      accessorFn: (row: ChapterTableRow) => Date.parse(row.chapter.date_uploaded || "") || 0,
      header: "app.manga.uploaded",
    },
    {
      id: "status",
      accessorFn: chapterStatusLabel,
      header: "app.downloads.status",
    },
    {
      id: "read",
      accessorFn: (row: ChapterTableRow) =>
        row.chapter.read_completed ? 2 : row.chapter.pages_read > 0 ? 1 : 0,
      header: "app.libraryDetail.readProgress",
    },
    {
      id: "progress",
      accessorFn: (row: ChapterTableRow) =>
        row.download?.progress ?? (row.chapter.downloaded ? 100 : 0),
      header: "app.downloads.progress",
    },
    {
      id: "actions",
      enableSorting: false,
      header: "app.manga.actions",
    },
  ];

  const chapterTable = createTable(
    {
      _features: _chapterTableFeatures,
      _rowModels: {
        sortedRowModel: createSortedRowModel(sortFns),
      },
      get data() {
        return chapterRows;
      },
      columns: chapterColumns,
      getRowId: (row: ChapterTableRow) => row.chapter.id,
    },
    (state) => state,
  );

  const sortedChapterRows = $derived(chapterTable.getRowModel().rows);
  const selectedChapterRows = $derived(getSelectedChapterRows());
  const selectedActionLoading = $derived(
    bulkDownloadLoading ||
      selectedDeleteLoading ||
      selectedReencodeLoading ||
      selectedReadLoading ||
      selectedUnreadLoading,
  );
  const chapterActionSelection = $derived(getChapterActionSelection(selectedChapterRows));
  const chapterActionAvailability = $derived(
    getChapterActionAvailability(chapterActionSelection, selectedActionLoading),
  );
  const selectedCount = $derived(chapterActionSelection.selectedCount);
  const selectedDownloadableChapters = $derived(chapterActionSelection.downloadableChapters);
  const selectedDownloads = $derived(chapterActionSelection.downloads);
  const selectedCompletedDownloads = $derived(chapterActionSelection.completedDownloads);
  const selectedReadableChapters = $derived(chapterActionSelection.readableChapters);
  const selectedUnreadableChapters = $derived(chapterActionSelection.unreadableChapters);
  const downloadActionDisabled = $derived(chapterActionAvailability.downloadDisabled);
  const reencodeActionDisabled = $derived(chapterActionAvailability.reencodeDisabled);
  const deleteActionDisabled = $derived(chapterActionAvailability.deleteDisabled);
  const markReadActionDisabled = $derived(chapterActionAvailability.markReadDisabled);
  const markUnreadActionDisabled = $derived(chapterActionAvailability.markUnreadDisabled);
  const clearActionDisabled = $derived(chapterActionAvailability.clearDisabled);

  function getSelectedChapterRows(): ChapterTableRow[] {
    return chapterTable.getSelectedRowModel().rows.map((row) => row.original);
  }

  async function saveCategory() {
    if (!effectiveSelectedCategory) {
      return;
    }
    savingCategory = true;
    categorySaveMessage = "";

    try {
      await saveLocalLibraryCategory(
        categoryMutation,
        queryClient,
        libraryId,
        effectiveSelectedCategory,
      );
      categorySaveMessage = translate("app.libraryDetail.categoryUpdated");
    } catch (error: unknown) {
      categorySaveMessage = errorMessage(error);
    } finally {
      savingCategory = false;
    }
  }

  async function downloadSelectedChapters() {
    if (!libraryId || bulkDownloadLoading) {
      return;
    }

    const chaptersToDownload = selectedDownloadableChapters;
    if (chaptersToDownload.length === 0) {
      chapterActionMessageIsError = false;
      chapterActionMessage =
        selectedCount === 0
          ? translate("app.libraryDetail.selectChaptersDownload")
          : translate("app.libraryDetail.alreadyDownloadedOrQueued");
      return;
    }

    chapterActionMessage = "";
    chapterActionMessageIsError = false;
    bulkDownloadLoading = true;

    try {
      const queuedCount = await queueLocalLibraryChaptersForDownload(
        downloadBulkMutation,
        libraryId,
        chaptersToDownload,
      );
      await Promise.all([
        invalidateLocalLibraryDownloads(queryClient),
        invalidateLocalLibraryState(queryClient, libraryId),
      ]);
      chapterTable.resetRowSelection();
      chapterActionMessage =
        queuedCount === 1
          ? translate("app.libraryDetail.queueDownloadOne")
          : translate("app.libraryDetail.queueDownload", { count: queuedCount });
    } catch (error: unknown) {
      chapterActionMessageIsError = true;
      chapterActionMessage = errorMessage(error);
    } finally {
      bulkDownloadLoading = false;
    }
  }

  function formatDownloadCount(count: number) {
    return translate("app.libraryDetail.downloadCount", { count });
  }

  function chapterStatusLabel(row: ChapterTableRow): string {
    if (row.download) {
      return getTranslatedDownloadDisplayLabel(row.download.status, translate);
    }

    return row.chapter.downloaded
      ? translate("app.manga.downloaded")
      : translate("app.libraryDetail.notDownloaded");
  }

  function chapterReadLabel(chapter: LibraryChapter): string {
    if (chapter.read_completed) {
      return translate("app.libraryDetail.readProgress");
    }

    if (chapter.pages_read > 0) {
      return translate("app.reader.pageAlt", { page: chapter.pages_read });
    }

    return translate("app.libraryDetail.unread");
  }

  function sortLabel(header: string | unknown): string {
    return typeof header === "string" ? translate(header) : "";
  }

  function chapterHeaderCellClass(columnId: string): string {
    if (columnId === "select") {
      return publicView ? "hidden" : "w-12 pl-5";
    }

    if (columnId === "actions") {
      return "w-24 pr-5 text-right";
    }

    if (columnId === "progress") {
      return "w-[180px]";
    }

    if (columnId === "read") {
      return "w-28";
    }

    return "";
  }

  function selectionBoxClass(selected: boolean): string {
    const base = "flex size-4 items-center justify-center border transition-colors";
    if (selected) {
      return `${base} border-primary bg-primary text-primary-foreground`;
    }

    return `${base} border-muted-foreground/45 bg-card text-transparent group-hover:border-muted-foreground`;
  }

  function disabledActionClass(disabled: boolean): string {
    return disabled ? "opacity-50" : "";
  }

  function updateSelectedCategory(value: string) {
    selectedCategory = value;
  }

  function runBulkAction(event: MouseEvent, disabled: boolean, action: () => void | Promise<void>) {
    if (disabled) {
      event.preventDefault();
      return;
    }

    void action();
  }

  function handleDownloadSelected(event: MouseEvent) {
    runBulkAction(event, downloadActionDisabled, downloadSelectedChapters);
  }

  function handleReencodeSelected(event: MouseEvent) {
    runBulkAction(event, reencodeActionDisabled, reencodeSelectedDownloads);
  }

  function handleMarkSelectedRead(event: MouseEvent) {
    runBulkAction(event, markReadActionDisabled, markSelectedRead);
  }

  function handleMarkSelectedUnread(event: MouseEvent) {
    runBulkAction(event, markUnreadActionDisabled, markSelectedUnread);
  }

  function handleDeleteSelected(event: MouseEvent) {
    runBulkAction(event, deleteActionDisabled, deleteSelectedDownloads);
  }

  function clearSelectedChapters() {
    chapterTable.resetRowSelection();
  }

  function handleClearSelection(event: MouseEvent) {
    runBulkAction(event, clearActionDisabled, clearSelectedChapters);
  }

  async function deleteSelectedDownloads() {
    if (!libraryId || selectedDeleteLoading) {
      return;
    }

    const downloadsToDelete = [...selectedDownloads];
    if (downloadsToDelete.length === 0) {
      chapterActionMessageIsError = false;
      chapterActionMessage =
        selectedCount === 0
          ? translate("app.libraryDetail.selectChaptersDelete")
          : translate("app.libraryDetail.selectedNoDownloads");
      return;
    }

    chapterActionMessage = "";
    chapterActionMessageIsError = false;
    selectedDeleteLoading = true;

    try {
      await deleteLocalLibraryDownloads(removeMutation, downloadsToDelete);
      await Promise.all([
        invalidateLocalLibraryDownloads(queryClient),
        invalidateLocalLibraryState(queryClient, libraryId),
      ]);
      chapterTable.resetRowSelection();
      chapterActionMessage = translate("app.libraryDetail.deletedDownloads", {
        count: formatDownloadCount(downloadsToDelete.length),
      });
    } catch (error: unknown) {
      chapterActionMessageIsError = true;
      chapterActionMessage = errorMessage(error);
    } finally {
      selectedDeleteLoading = false;
    }
  }

  async function reencodeSelectedDownloads() {
    if (!libraryId || selectedReencodeLoading) {
      return;
    }

    const downloadsToReencode = [...selectedCompletedDownloads];
    if (downloadsToReencode.length === 0) {
      chapterActionMessageIsError = false;
      chapterActionMessage =
        selectedCount === 0
          ? translate("app.libraryDetail.selectDownloadedReencode")
          : translate("app.libraryDetail.selectedNoCompletedDownloads");
      return;
    }

    chapterActionMessage = "";
    chapterActionMessageIsError = false;
    selectedReencodeLoading = true;

    try {
      await reencodeLocalLibraryDownloads(reencodeMutation, downloadsToReencode);
      await Promise.all([
        invalidateLocalLibraryDownloads(queryClient),
        invalidateLocalLibraryState(queryClient, libraryId),
      ]);
      chapterTable.resetRowSelection();
      chapterActionMessage =
        downloadsToReencode.length === 1
          ? translate("app.libraryDetail.queueReencodeOne")
          : translate("app.libraryDetail.queueReencode", { count: downloadsToReencode.length });
    } catch (error: unknown) {
      chapterActionMessageIsError = true;
      chapterActionMessage = errorMessage(error);
    } finally {
      selectedReencodeLoading = false;
    }
  }

  async function markSelectedRead() {
    if (!libraryId || selectedReadLoading) {
      return;
    }

    const chaptersToMark = [...selectedReadableChapters];
    if (chaptersToMark.length === 0) {
      chapterActionMessageIsError = false;
      chapterActionMessage = translate("app.libraryDetail.selectChaptersMarkRead");
      return;
    }

    chapterActionMessage = "";
    chapterActionMessageIsError = false;
    selectedReadLoading = true;

    try {
      await markLocalLibraryChaptersRead(updateReadProgressMutation, chaptersToMark);
      await invalidateLocalLibraryState(queryClient, libraryId);
      chapterTable.resetRowSelection();
      chapterActionMessage =
        chaptersToMark.length === 1
          ? translate("app.libraryDetail.markedReadOne")
          : translate("app.libraryDetail.markedRead", { count: chaptersToMark.length });
    } catch (error: unknown) {
      chapterActionMessageIsError = true;
      chapterActionMessage = errorMessage(error);
    } finally {
      selectedReadLoading = false;
    }
  }

  async function markSelectedUnread() {
    if (!libraryId || selectedUnreadLoading) {
      return;
    }

    const chaptersToClear = [...selectedUnreadableChapters];
    if (chaptersToClear.length === 0) {
      chapterActionMessageIsError = false;
      chapterActionMessage =
        selectedCount === 0
          ? translate("app.libraryDetail.selectChaptersMarkUnread")
          : translate("app.libraryDetail.alreadyUnread");
      return;
    }

    chapterActionMessage = "";
    chapterActionMessageIsError = false;
    selectedUnreadLoading = true;

    try {
      await markLocalLibraryChaptersUnread(clearReadProgressMutation, chaptersToClear);
      await invalidateLocalLibraryState(queryClient, libraryId);
      chapterTable.resetRowSelection();
      chapterActionMessage =
        chaptersToClear.length === 1
          ? translate("app.libraryDetail.markedUnreadOne")
          : translate("app.libraryDetail.markedUnread", { count: chaptersToClear.length });
    } catch (error: unknown) {
      chapterActionMessageIsError = true;
      chapterActionMessage = errorMessage(error);
    } finally {
      selectedUnreadLoading = false;
    }
  }

  async function refreshChapters() {
    refreshMessage = "";
    refreshMessageIsError = false;

    try {
      await refreshLocalLibraryChapters(refreshChaptersMutation, libraryId);
      await chaptersQuery.refetch();
      refreshMessage = translate("app.libraryDetail.refreshedChapters");
    } catch (error: unknown) {
      refreshMessageIsError = true;
      refreshMessage = errorMessage(error);
    }
  }

  async function refreshComicInfo() {
    comicInfoRefreshMessage = "";
    comicInfoRefreshMessageIsError = false;

    try {
      const filesRewritten = await refreshLocalLibraryComicInfo(
        refreshComicInfoMutation,
        queryClient,
        libraryId,
      );
      comicInfoRefreshMessage =
        filesRewritten === 1
          ? translate("app.libraryDetail.refreshedComicInfoOne")
          : translate("app.libraryDetail.refreshedComicInfo", { count: filesRewritten });
    } catch (error: unknown) {
      comicInfoRefreshMessageIsError = true;
      comicInfoRefreshMessage = errorMessage(error);
    }
  }

  async function removeFromLibrary() {
    removeMessage = "";
    removeMessageIsError = false;

    if (!confirm(translate("app.libraryDetail.confirmRemove"))) {
      return;
    }

    try {
      await removeLocalLibraryManga(removeFromLibraryMutation, queryClient, libraryId);
      removeMessage = translate("app.libraryDetail.removed");
      setTimeout(() => {
        void goto(resolve("/" as const));
      }, 1000);
    } catch (error: unknown) {
      removeMessageIsError = true;
      removeMessage = errorMessage(error);
    }
  }

  return {
    get availableCategories() {
      return availableCategories;
    },
    get bulkDownloadLoading() {
      return bulkDownloadLoading;
    },
    get categorySaveMessage() {
      return categorySaveMessage;
    },
    get chapterActionMessage() {
      return chapterActionMessage;
    },
    get chapterActionMessageIsError() {
      return chapterActionMessageIsError;
    },
    get chapterColumns() {
      return chapterColumns;
    },
    chapterHeaderCellClass,
    chapterReadLabel,
    get chapterTable() {
      return chapterTable;
    },
    get chaptersError() {
      return chaptersError;
    },
    get chaptersLoading() {
      return chaptersLoading;
    },
    get clearActionDisabled() {
      return clearActionDisabled;
    },
    get comicInfoRefreshMessage() {
      return comicInfoRefreshMessage;
    },
    get comicInfoRefreshMessageIsError() {
      return comicInfoRefreshMessageIsError;
    },
    get comicInfoRows() {
      return comicInfoRows;
    },
    get deleteActionDisabled() {
      return deleteActionDisabled;
    },
    disabledActionClass,
    get downloadActionDisabled() {
      return downloadActionDisabled;
    },
    get effectiveSelectedCategory() {
      return effectiveSelectedCategory;
    },
    handleClearSelection,
    handleDeleteSelected,
    handleDownloadSelected,
    handleMarkSelectedRead,
    handleMarkSelectedUnread,
    handleReencodeSelected,
    get manga() {
      return manga;
    },
    get markReadActionDisabled() {
      return markReadActionDisabled;
    },
    get markUnreadActionDisabled() {
      return markUnreadActionDisabled;
    },
    get pageError() {
      return pageError;
    },
    get pageLoading() {
      return pageLoading;
    },
    get reencodeActionDisabled() {
      return reencodeActionDisabled;
    },
    refreshChapters,
    get refreshChaptersMutation() {
      return refreshChaptersMutation;
    },
    refreshComicInfo,
    get refreshComicInfoMutation() {
      return refreshComicInfoMutation;
    },
    get refreshMessage() {
      return refreshMessage;
    },
    get refreshMessageIsError() {
      return refreshMessageIsError;
    },
    removeFromLibrary,
    get removeFromLibraryMutation() {
      return removeFromLibraryMutation;
    },
    get removeMessage() {
      return removeMessage;
    },
    get removeMessageIsError() {
      return removeMessageIsError;
    },
    saveCategory,
    get savingCategory() {
      return savingCategory;
    },
    get selectedCount() {
      return selectedCount;
    },
    get selectedDeleteLoading() {
      return selectedDeleteLoading;
    },
    get selectedReadLoading() {
      return selectedReadLoading;
    },
    get selectedReencodeLoading() {
      return selectedReencodeLoading;
    },
    get selectedUnreadLoading() {
      return selectedUnreadLoading;
    },
    selectionBoxClass,
    get settingsQuery() {
      return settingsQuery;
    },
    sortLabel,
    get sortedChapterRows() {
      return sortedChapterRows;
    },
    updateSelectedCategory,
  };
}
