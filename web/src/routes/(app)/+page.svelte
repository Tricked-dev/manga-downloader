<script lang="ts">
  import { createGetDownloads } from "@manga-server/api-client/generated/endpoints/downloads";
  import {
    createGetLibrary,
    getGetLibraryQueryKey,
  } from "@manga-server/api-client/generated/endpoints/library";
  import { Alert, AlertDescription, AlertTitle } from "$lib/ui/alert";
  import { Button } from "$lib/ui/button";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { Library as LibraryIcon } from "@lucide/svelte";
  import { untrack } from "svelte";
  import PageHeader from "@manga-server/ui/components/page-header";
  import EmptyState from "$components/common/EmptyState.svelte";
  import LibraryCollectionGrid from "$components/features/library/LibraryCollectionGrid.svelte";
  import LibraryFilters from "$components/features/library/LibraryFilters.svelte";
  import LibraryLoadingGrid from "$components/features/library/LibraryLoadingGrid.svelte";
  import { getLibraryView } from "$lib/features/library/library-view";
  import { t } from "$lib/i18n";
  import { QUERY_CACHE_TIMES } from "$lib/query-client";
  import { dehydratedQueryOptions } from "$lib/query-hydration";
  import { selectItems } from "$lib/query-selectors";
  import type { DownloadItem, LibraryManga } from "$lib/types";
  import { getErrorMessage } from "$lib/utils";
  import { useHydrate, useQueryClient, type DehydratedState } from "@tanstack/svelte-query";

  const { data } = $props<{
    data: {
      dehydratedState?: DehydratedState;
    };
  }>();
  const queryClient = useQueryClient();

  useHydrate(untrack(() => data.dehydratedState), undefined, queryClient);

  const LIBRARY_WINDOW_SIZE = 48;
  const EMPTY_LIBRARY: LibraryManga[] = [];
  const EMPTY_DOWNLOADS: DownloadItem[] = [];

  const libraryQuery = createGetLibrary(() => ({
    query: {
      ...dehydratedQueryOptions(data.dehydratedState, getGetLibraryQueryKey()),
      select: selectItems<LibraryManga>,
      staleTime: QUERY_CACHE_TIMES.stable,
    },
  }));

  const downloadsQuery = createGetDownloads(undefined, () => ({
    query: {
      select: selectItems<DownloadItem>,
      staleTime: QUERY_CACHE_TIMES.active,
    },
  }));

  const library: LibraryManga[] = $derived(libraryQuery.data ?? EMPTY_LIBRARY);
  const downloads: DownloadItem[] = $derived(downloadsQuery.data ?? EMPTY_DOWNLOADS);
  const loading = $derived(libraryQuery.isPending);
  const error = $derived(libraryQuery.error ? getErrorMessage(libraryQuery.error, $t("app.library.requestFailed")) : "");
  let selectedCategory = $state("all");
  let selectedAgeRating = $state("all");
  let selectedGenre = $state("all");
  let selectedLanguage = $state("all");
  let selectedStatus = $state("all");
  let searchQuery = $state("");
  let visibleLibraryCount = $state(LIBRARY_WINDOW_SIZE);

  const libraryView = $derived(getCurrentLibraryView());

  const ageRatingOptions: string[] = $derived(libraryView.ageRatingOptions);
  const categoryOptions: string[] = $derived(libraryView.categoryOptions);
  const genreOptions: string[] = $derived(libraryView.genreOptions);
  const languageOptions: string[] = $derived(libraryView.languageOptions);
  const statusOptions: string[] = $derived(libraryView.statusOptions);
  const filteredLibrary: LibraryManga[] = $derived(libraryView.filteredLibrary);
  const visibleLibrary: LibraryManga[] = $derived(getVisibleLibrary());
  const downloadsByManga = $derived(libraryView.downloadsByManga);
  const activeLibraryFilters: string[] = $derived(getActiveLibraryFilters());

  function getCurrentLibraryView() {
    return getLibraryView(library, downloads, selectedCategory, {
      ageRating: selectedAgeRating,
      genre: selectedGenre,
      language: selectedLanguage,
      search: searchQuery,
      status: selectedStatus,
    });
  }

  function getVisibleLibrary() {
    return filteredLibrary.slice(0, visibleLibraryCount);
  }

  function getActiveLibraryFilters(): string[] {
    const filters: string[] = [];
    const trimmedSearch = searchQuery.trim();

    if (trimmedSearch) {
      filters.push($t("app.library.filters.search", { value: trimmedSearch }));
    }
    if (selectedCategory !== "all") {
      filters.push($t("app.library.filters.category", { value: selectedCategory }));
    }
    if (selectedStatus !== "all") {
      filters.push($t("app.library.filters.status", { value: selectedStatus }));
    }
    if (selectedGenre !== "all") {
      filters.push($t("app.library.filters.genre", { value: selectedGenre }));
    }
    if (selectedLanguage !== "all") {
      filters.push($t("app.library.filters.language", { value: selectedLanguage }));
    }
    if (selectedAgeRating !== "all") {
      filters.push($t("app.library.filters.rating", { value: selectedAgeRating }));
    }

    return filters;
  }
  const hasActiveLibraryFilters = $derived(activeLibraryFilters.length > 0);
  function setSelectedCategory(category: string) {
    selectedCategory = category;
    visibleLibraryCount = LIBRARY_WINDOW_SIZE;
  }

  function setLibraryFilter(name: "ageRating" | "genre" | "language" | "status", value: string) {
    if (name === "ageRating") {
      selectedAgeRating = value;
    } else if (name === "genre") {
      selectedGenre = value;
    } else if (name === "language") {
      selectedLanguage = value;
    } else {
      selectedStatus = value;
    }
    visibleLibraryCount = LIBRARY_WINDOW_SIZE;
  }

  function updateSearchQuery(value: string) {
    searchQuery = value;
    visibleLibraryCount = LIBRARY_WINDOW_SIZE;
  }

  function handleSearchInput(event: Event) {
    updateSearchQuery((event.currentTarget as HTMLInputElement).value);
  }

  function setStatusFilter(value: string) {
    setLibraryFilter("status", value);
  }

  function setGenreFilter(value: string) {
    setLibraryFilter("genre", value);
  }

  function setLanguageFilter(value: string) {
    setLibraryFilter("language", value);
  }

  function setAgeRatingFilter(value: string) {
    setLibraryFilter("ageRating", value);
  }

  function resetLibraryFilters() {
    searchQuery = "";
    selectedCategory = "all";
    selectedAgeRating = "all";
    selectedGenre = "all";
    selectedLanguage = "all";
    selectedStatus = "all";
    visibleLibraryCount = LIBRARY_WINDOW_SIZE;
  }

  function loadMoreLibrary() {
    visibleLibraryCount = Math.min(
      visibleLibraryCount + LIBRARY_WINDOW_SIZE,
      filteredLibrary.length,
    );
  }
</script>

<svelte:head>
  <title>{$t("app.library.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<div class="space-y-6">
  <PageHeader
    title={$t("app.library.title")}
    description={$t("app.library.description")}
    innerClass="lg:flex-col lg:items-stretch"
    actionsClass="w-full pt-2 sm:items-stretch"
  >
    {#snippet actions()}
      <LibraryFilters
        {searchQuery}
        {selectedCategory}
        {selectedAgeRating}
        {selectedGenre}
        {selectedLanguage}
        {selectedStatus}
        {categoryOptions}
        {ageRatingOptions}
        {genreOptions}
        {languageOptions}
        {statusOptions}
        activeFilters={activeLibraryFilters}
        hasActiveFilters={hasActiveLibraryFilters}
        onSearchInput={handleSearchInput}
        onSelectCategory={setSelectedCategory}
        onSelectStatus={setStatusFilter}
        onSelectGenre={setGenreFilter}
        onSelectLanguage={setLanguageFilter}
        onSelectAgeRating={setAgeRatingFilter}
        onReset={resetLibraryFilters}
      />
    {/snippet}
  </PageHeader>

  {#if loading}
    <LibraryLoadingGrid />
  {:else if error}
    <Alert variant="destructive">
      <AlertTitle>{$t("app.library.libraryUnavailable")}</AlertTitle>
      <AlertDescription>
        <p>{$t("app.library.serverPortHint")}</p>
        <p>{error}</p>
      </AlertDescription>
    </Alert>
  {:else if library.length === 0}
    <EmptyState
      icon={LibraryIcon}
      title={$t("app.library.emptyTitle")}
      description={$t("app.library.emptyDescription")}
    />
  {:else if filteredLibrary.length === 0}
    <Card class="gap-0">
      <CardHeader class="border-b border-border/70">
        <CardTitle>{$t("app.library.collection")}</CardTitle>
        <CardDescription>{$t("app.library.matchingCurrentFilters", { count: filteredLibrary.length })}</CardDescription>
      </CardHeader>
      <CardContent>
        <EmptyState
          icon={LibraryIcon}
          title={$t("app.library.noMatchesTitle")}
          description={$t("app.library.noMatchesDescription")}
        />
        {#if hasActiveLibraryFilters}
          <div class="border-x border-b border-dashed border-border/70 bg-card px-6 pb-8 text-center">
            <Button variant="secondary" onclick={resetLibraryFilters}>{$t("app.actions.clearFilters")}</Button>
          </div>
        {/if}
      </CardContent>
    </Card>
  {:else}
    <LibraryCollectionGrid
      filteredCount={filteredLibrary.length}
      {visibleLibrary}
      totalVisibleCount={filteredLibrary.length}
      {downloadsByManga}
      onLoadMore={loadMoreLibrary}
    />
  {/if}
</div>
