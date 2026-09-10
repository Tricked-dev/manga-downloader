<script lang="ts">
  import { browser } from "$app/environment";
  import {
    createAddToLibrary,
    createEnqueueDownload,
    createGetChapterList,
    createGetMangaDetails,
    createGetSettings,
    getGetDownloadsQueryKey,
    getGetLibraryQueryKey,
  } from "@manga-server/api-client/generated";
  import {
    createSortedRowModel,
    createTable,
    rowSortingFeature,
    sortFns,
    tableFeatures,
    type ColumnDef,
  } from "@tanstack/svelte-table";
  import { getErrorMessage, parseCsvSetting } from "$lib/utils";
  import { useQueryClient } from "@tanstack/svelte-query";
  import { page } from "$app/state";
  import { SvelteSet } from "svelte/reactivity";
  import { Card, CardContent } from "$lib/ui/card";
  import PageHeader from "@manga-server/ui/components/page-header";
  import SourceMangaChaptersTable from "$components/features/manga/SourceMangaChaptersTable.svelte";
  import SourceMangaHeroActions from "$components/features/manga/SourceMangaHeroActions.svelte";
  import SourceMangaLoading from "$components/features/manga/SourceMangaLoading.svelte";
  import MangaHero from "$components/manga/MangaHero.svelte";
  import { t } from "$lib/i18n";
  import { QUERY_CACHE_TIMES } from "$lib/query-client";
  import { selectItems, selectSettings } from "$lib/query-selectors";
  import type { SourceChapterData } from "@manga-server/api-client/models";

  const EMPTY_CHAPTERS: SourceChapterData[] = [];
  const EMPTY_CATEGORIES: string[] = [];

  type ChapterTableRow = {
    chapter: SourceChapterData;
    downloading: boolean;
    queued: boolean;
  };

  const queryClient = useQueryClient();

  const source = $derived(page.params.sourceId);
  const mangaId = $derived(page.params.remoteSeriesId);
  const sourceName = $derived(source ?? "");
  const mangaSourceId = $derived(mangaId ?? "");
  const mangaQuery = createGetMangaDetails(
    () => sourceName,
    () => mangaSourceId,
    () => ({
      query: {
        enabled: browser && sourceName.length > 0 && mangaSourceId.length > 0,
        staleTime: QUERY_CACHE_TIMES.stable,
      },
    }),
  );
  const resolvedMangaSourceId = $derived(mangaQuery.data?.id ?? mangaSourceId);
  const chaptersQuery = createGetChapterList(
    () => sourceName,
    () => resolvedMangaSourceId,
    () => ({
      query: {
        enabled: browser && sourceName.length > 0 && resolvedMangaSourceId.length > 0,
        select: selectItems<SourceChapterData>,
        staleTime: QUERY_CACHE_TIMES.stable,
      },
    }),
  );

  const manga = $derived(mangaQuery.data ?? null);
  const chapters = $derived(chaptersQuery.data ?? EMPTY_CHAPTERS);
  const pageLoading = $derived(!manga && mangaQuery.isPending);
  const pageError = $derived(
    mangaQuery.error ? getErrorMessage(mangaQuery.error, $t("app.manga.failedToLoad")) : ""
  );
  const chaptersLoading = $derived(chaptersQuery.isPending && chapters.length === 0);
  const chaptersError = $derived(
    chaptersQuery.error ? getErrorMessage(chaptersQuery.error, $t("app.manga.failedToLoadChapters")) : ""
  );
  let addingToLibrary = $state(false);
  let addedToLibrary = $state(false);
  const downloadingChapters = new SvelteSet<string>();
  const queuedChapters = new SvelteSet<string>();
  let selectedCategory = $state("");
  let actionMessage = $state("");
  const categorySettingsQuery = createGetSettings(() => ({
    query: {
      enabled: browser,
      select: selectSettings,
      staleTime: QUERY_CACHE_TIMES.stable,
    },
  }));

  const libraryCategories = $derived(
    parseCsvSetting(categorySettingsQuery.data?.library_categories, EMPTY_CATEGORIES)
  );
  const effectiveSelectedCategory = $derived(selectedCategory || libraryCategories[0] || "");

  function errorMessage(error: unknown) {
    return getErrorMessage(error, $t("app.errors.genericShort"));
  }

  const chapterRows = $derived(getChapterRows(chapters));

  const _chapterTableFeatures = tableFeatures({
    rowSortingFeature,
  });
  const chapterColumns: ColumnDef<typeof _chapterTableFeatures, ChapterTableRow>[] = [
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

  function getChapterRows(chapterItems: SourceChapterData[]): ChapterTableRow[] {
    return chapterItems.map((chapter: SourceChapterData) => ({
      chapter,
      downloading: downloadingChapters.has(chapter.id),
      queued: queuedChapters.has(chapter.id),
    }));
  }

  function updateSelectedCategory(value: string) {
    selectedCategory = value;
  }

  const addToLibraryMutation = createAddToLibrary();

  const downloadMutation = createEnqueueDownload();

  function getAddToLibraryPayload() {
    if (!manga) {
      return null;
    }

    return {
      author: manga.author,
      category: effectiveSelectedCategory,
      cover_fetch_spec: manga.cover_fetch_spec,
      cover_url: manga.cover_url,
      description: manga.description,
      genres: [...(manga.genres ?? [])],
      source: sourceName,
      source_id: resolvedMangaSourceId,
      status: manga.status,
      title: manga.title,
    };
  }

  async function addCurrentMangaToLibrary() {
    const payload = getAddToLibraryPayload();
    if (!payload) {
      throw new Error($t("app.manga.mangaNotLoaded"));
    }

    await addToLibraryMutation.mutateAsync({ data: payload });
    await queryClient.invalidateQueries({ queryKey: getGetLibraryQueryKey() });
    addedToLibrary = true;
  }

  async function addToLibrary() {
    addingToLibrary = true;
    actionMessage = "";
    try {
      await addCurrentMangaToLibrary();
    } catch (error: unknown) {
      actionMessage = errorMessage(error);
    } finally {
      addingToLibrary = false;
    }
  }

  async function downloadChapter(chapterId: string) {
    actionMessage = "";
    downloadingChapters.add(chapterId);
    try {
      if (!addedToLibrary) {
        await addCurrentMangaToLibrary();
      }

      await downloadMutation.mutateAsync({ data: { chapter_id: chapterId, manga_id: resolvedMangaSourceId } });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: getGetDownloadsQueryKey() }),
        queryClient.invalidateQueries({ queryKey: getGetLibraryQueryKey() }),
      ]);
      queuedChapters.add(chapterId);
    } catch (error: unknown) {
      actionMessage = errorMessage(error);
    } finally {
      downloadingChapters.delete(chapterId);
    }
  }

  function chapterStatusLabel(row: ChapterTableRow) {
    if (row.downloading) {
      return $t("app.manga.downloading");
    }

    return row.queued ? $t("app.manga.queued") : $t("app.manga.available");
  }

  function sortLabel(header: string | unknown): string {
    return typeof header === "string" ? $t(header) : "";
  }

  function chapterHeaderCellClass(columnId: string): string {
    return columnId === "actions" ? "w-[220px] pr-5 text-right" : "";
  }

</script>

<svelte:head>
  <title>{manga?.title ?? $t("app.manga.loadingTitle")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<div class="space-y-8">
  <PageHeader
    title={manga?.title ?? $t("app.manga.loadingTitle")}
    description={$t("app.manga.description")}
    backHref={`/sources/${encodeURIComponent(sourceName)}/search`}
    backLabel={$t("app.manga.backToSearch")}
  />

  {#if pageLoading}
    <SourceMangaLoading {sourceName} />
  {:else if pageError}
    <Card class="border-destructive/30 bg-destructive/5">
      <CardContent class="p-4">
        <p class="text-sm font-medium text-destructive">{pageError}</p>
      </CardContent>
    </Card>
  {:else if manga}
    <MangaHero
      title={manga.title}
      coverUrl={manga.cover_proxy_url ?? manga.cover_url}
      coverBaseUrl={manga.source_base_url}
      author={manga.author}
      status={manga.status}
      description={manga.description}
    >
      {#snippet actions()}
        <SourceMangaHeroActions
          {addedToLibrary}
          {addingToLibrary}
          categories={libraryCategories}
          categoryLoading={categorySettingsQuery.isPending}
          selectedCategory={effectiveSelectedCategory}
          onAddToLibrary={addToLibrary}
          onSelectCategory={updateSelectedCategory}
        />
      {/snippet}
    </MangaHero>

    <SourceMangaChaptersTable
      {actionMessage}
      {chapterColumns}
      {chapterHeaderCellClass}
      {chapterTable}
      {chaptersError}
      {chaptersLoading}
      count={chapters.length}
      onDownloadChapter={downloadChapter}
      {sortLabel}
      {sortedChapterRows}
      {sourceName}
    />
  {/if}
</div>
