<script lang="ts">
  import { browser } from "$app/environment";
  import { createSearchSourceInfinite, searchSource } from "@manga-server/api-client/generated";
  import type { InfiniteData } from "@tanstack/svelte-query";
  import { Input } from "$lib/ui/input";
  import { Button } from "$lib/ui/button";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { Select, SelectContent, SelectItem, SelectTrigger } from "$lib/ui/select";
  import { LoaderCircle, Search } from "@lucide/svelte";
  import PageHeader from "@manga-server/ui/components/page-header";
  import LoadingState from "$components/common/LoadingState.svelte";
  import EmptyState from "$components/common/EmptyState.svelte";
  import InfiniteScrollTrigger from "$components/common/InfiniteScrollTrigger.svelte";
  import MangaCard from "$components/manga/MangaCard.svelte";
  import { QUERY_CACHE_TIMES } from "$lib/query-client";
  import { t } from "$lib/i18n";
  import type { SearchResult, Source } from "$lib/types";
  import { getErrorMessage } from "$lib/utils";
  import type { SearchResponse } from "@manga-server/api-client/generated";

  const SEARCH_RESULT_WINDOW_SIZE = 36;
  const EMPTY_SEARCH_CATEGORIES: string[] = [];

  const { sourceName, sourceInfo }: { sourceName: string; sourceInfo: Source | null } = $props();
  const initialCategory = getInitialCategory();

  let query = $state("");
  let selectedCategory = $state(initialCategory);
  let submittedQuery = $state("");
  let submittedCategory = $state(initialCategory);
  let searched = $state(Boolean(initialCategory));
  let visibleResultCount = $state(SEARCH_RESULT_WINDOW_SIZE);
  const availableFilters = $derived(sourceInfo?.search_categories ?? EMPTY_SEARCH_CATEGORIES);

  const searchResultsQuery = createSearchSourceInfinite<InfiniteData<SearchResult>, Error>(
    () => sourceName,
    () => ({
      category: submittedCategory || undefined,
      q: submittedQuery,
    }),
    () => ({
      query: {
        enabled: browser && searched && Boolean(sourceName) && Boolean(submittedQuery || submittedCategory),
        getNextPageParam: (lastPage: SearchResponse, _allPages: SearchResponse[], lastPageParam: unknown) =>
          lastPage.has_next_page ? Number(lastPageParam) + 1 : undefined,
        initialPageParam: 1,
        queryFn: ({ pageParam }: { pageParam: unknown }) =>
          searchSource(sourceName, {
            category: submittedCategory || undefined,
            page: Number(pageParam),
            q: submittedQuery,
          }),
        staleTime: QUERY_CACHE_TIMES.stable,
      },
    }),
  );

  const results = $derived(getSearchResults());
  const visibleResults = $derived(getVisibleResults());
  const loading = $derived(searchResultsQuery.isPending);
  const loadingMore = $derived(searchResultsQuery.isFetchingNextPage);
  const hasNextPage = $derived(searchResultsQuery.hasNextPage ?? false);
  const error = $derived(
    searchResultsQuery.isError ? getErrorMessage(searchResultsQuery.error, $t("app.sources.search.requestFailed")) : "",
  );

  function submitSearch(nextQuery: string, nextCategory: string) {
    if ((!nextQuery && !nextCategory) || !sourceName) {return;}

    searched = true;
    submittedQuery = nextQuery;
    submittedCategory = nextCategory;
    visibleResultCount = SEARCH_RESULT_WINDOW_SIZE;
  }

  async function search() {
    const nextQuery = query.trim();
    submitSearch(nextQuery, selectedCategory);
  }

  async function applyCategory(value: string) {
    selectedCategory = value;
    if (!value && !query.trim()) {return;}
    submitSearch(query.trim(), value);
  }

  async function loadMore() {
    if (!hasNextPage || loadingMore) {return;}
    await searchResultsQuery.fetchNextPage();
  }

  async function loadMoreVisibleResults() {
    if (visibleResults.length < results.length) {
      visibleResultCount = Math.min(visibleResultCount + SEARCH_RESULT_WINDOW_SIZE, results.length);
      return;
    }

    await loadMore();
  }

  function handleSubmit(event: SubmitEvent) {
    event.preventDefault();
    void search();
  }

  function handleQueryInput(event: Event): void {
    query = (event.currentTarget as HTMLInputElement).value;
  }

  function handleLoadMoreVisibleResults(): void {
    void loadMoreVisibleResults();
  }

  function getInitialCategory() {
    return sourceInfo?.default_search_category?.trim() ?? "";
  }

  function getSearchResults(): SearchResult["mangas"] {
    return searchResultsQuery.data?.pages.flatMap((page: SearchResponse) => page.mangas) ?? [];
  }

  function getVisibleResults(): SearchResult["mangas"] {
    return results.slice(0, visibleResultCount);
  }

  function formatCategoryLabel(category: string): string {
    const labels: Record<string, string> = {
      average_score: $t("app.sources.search.highestRated"),
      best_match: $t("app.sources.search.bestMatch"),
      created_date: $t("app.sources.search.recentlyAdded"),
      most_follows: $t("app.sources.search.mostFollowed"),
      most_views_1mo: $t("app.sources.search.popular30"),
      most_views_7d: $t("app.sources.search.popular7"),
      title_ascending: $t("app.sources.search.titleAsc"),
      total_views: $t("app.sources.search.mostViewed"),
      updated_date: $t("app.sources.search.recentlyUpdated"),
      year_descending: $t("app.sources.search.yearNewest"),
    };
    return labels[category] ?? category.split("_").map(word => 
      word.charAt(0).toUpperCase() + word.slice(1)
    ).join(" ");
  }
</script>

<div class="space-y-8">
  <PageHeader
    title={sourceName}
    description={$t("app.sources.search.browseDescription")}
    backHref="/sources"
    backLabel={$t("app.sources.search.back")}
    innerClass="lg:flex-col lg:items-stretch"
    actionsClass="w-full pt-2 sm:items-stretch"
  >
    {#snippet actions()}
      <form
        class={availableFilters.length > 0
          ? "grid gap-2 md:grid-cols-[minmax(16rem,1fr)_12rem_auto]"
          : "grid gap-2 md:grid-cols-[minmax(16rem,1fr)_auto]"}
        onsubmit={handleSubmit}
      >
        <div class="relative flex-1">
          <Search class="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              type="text"
              value={query}
              oninput={handleQueryInput}
              placeholder={$t("app.sources.search.searchPlaceholder")}
              class="h-9 pl-9"
          />
        </div>

        {#if availableFilters.length > 0}
          <Select type="single" value={selectedCategory} onValueChange={applyCategory}>
            <SelectTrigger id="source-filter" class="h-9 w-full" aria-label={$t("app.sources.search.sortAria")}>
              {formatCategoryLabel(selectedCategory) || $t("app.sources.search.chooseCategory")}
            </SelectTrigger>
            <SelectContent>
              {#each availableFilters as filter (filter)}
                <SelectItem value={filter}>{formatCategoryLabel(filter)}</SelectItem>
              {/each}
            </SelectContent>
          </Select>
        {/if}

        <Button
          type="submit"
          variant="outline"
          disabled={loading || loadingMore || (!query.trim() && !selectedCategory)}
          class="h-9 w-36 px-4"
        >
          <span class="grid w-full grid-cols-[1rem_minmax(0,1fr)] items-center gap-1.5">
            {#if loading && !loadingMore}
              <LoaderCircle class="size-4 animate-spin" />
            {:else}
              <Search class="size-4" />
            {/if}
            <span class="grid justify-items-start">
              <span class="invisible col-start-1 row-start-1" aria-hidden="true">
                {$t("app.sources.search.searching")}
              </span>
              <span class="col-start-1 row-start-1">
                {loading && !loadingMore ? $t("app.sources.search.searching") : $t("app.sources.search.search")}
              </span>
            </span>
          </span>
        </Button>
      </form>
    {/snippet}
  </PageHeader>

  {#if error}
    <Card class="border-destructive/30 bg-destructive/5">
      <CardContent class="p-4">
        <p class="text-sm font-medium text-destructive">{error}</p>
      </CardContent>
    </Card>
  {/if}

  {#if loading}
    <LoadingState label={$t("app.sources.search.loading")} compact />
  {:else if searched && results.length === 0}
    <EmptyState
      icon={Search}
      title={$t("app.sources.search.emptyTitle")}
      description={$t("app.sources.search.emptyDescription")}
    />
  {:else if results.length > 0}
    <Card class="gap-0">
      <CardHeader class="border-b border-border/70">
        <CardTitle>{$t("app.sources.search.results")}</CardTitle>
        <CardDescription>{$t("app.sources.search.resultsLoaded", { count: results.length, source: sourceName })}</CardDescription>
      </CardHeader>
      <CardContent class="space-y-6 p-6">
        <div class="grid grid-cols-2 gap-6 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
          {#each visibleResults as manga (manga.id)}
            <MangaCard
              manga={manga}
              href={`/manga/${encodeURIComponent(sourceName)}/${encodeURIComponent(manga.id)}`}
              coverFormat="avif"
              fetchPriority="low"
            />
          {/each}
        </div>

        <InfiniteScrollTrigger
          hasMore={visibleResults.length < results.length || hasNextPage}
          label={$t("app.sources.search.loadingMore")}
          onLoadMore={handleLoadMoreVisibleResults}
        />
      </CardContent>
    </Card>
  {/if}
</div>
