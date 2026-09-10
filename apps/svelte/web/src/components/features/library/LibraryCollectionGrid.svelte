<script lang="ts">
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import InfiniteScrollTrigger from "$components/common/InfiniteScrollTrigger.svelte";
  import LibraryMangaGridCard from "$components/features/library/LibraryMangaGridCard.svelte";
  import { t } from "$lib/i18n";
  import type { DownloadItem, LibraryManga } from "$lib/types";

  let {
    filteredCount,
    visibleLibrary,
    totalVisibleCount,
    downloadsByManga,
    onLoadMore,
  } = $props<{
    filteredCount: number;
    visibleLibrary: LibraryManga[];
    totalVisibleCount: number;
    downloadsByManga: Record<string, DownloadItem[]>;
    onLoadMore: () => void;
  }>();
</script>

<Card class="gap-0">
  <CardHeader class="border-b border-border/70">
    <CardTitle>{$t("app.library.collection")}</CardTitle>
    <CardDescription>{$t("app.library.matchingCurrentFilters", { count: filteredCount })}</CardDescription>
  </CardHeader>
  <CardContent class="p-4">
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
      {#each visibleLibrary as manga (manga.id)}
        <LibraryMangaGridCard
          {manga}
          downloads={downloadsByManga[manga.id] ?? []}
        />
      {/each}
    </div>
    <InfiniteScrollTrigger
      hasMore={visibleLibrary.length < totalVisibleCount}
      label={$t("app.library.loadingMore")}
      onLoadMore={onLoadMore}
    />
  </CardContent>
</Card>
