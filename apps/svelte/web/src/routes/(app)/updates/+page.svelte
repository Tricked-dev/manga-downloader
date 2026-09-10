<script lang="ts">
  import {
    createGetLibrary,
    createGetLibraryUpdates,
    createTriggerLibraryUpdate,
    getGetLibraryQueryKey,
    getGetLibraryUpdatesQueryKey,
  } from "@manga-server/api-client/generated";
  import EmptyState from "$components/common/EmptyState.svelte";
  import PageHeader from "@manga-server/ui/components/page-header";
  import { Alert, AlertDescription, AlertTitle } from "$lib/ui/alert";
  import { Button } from "$lib/ui/button";
  import UpdatesFeedCard from "$components/features/updates/UpdatesFeedCard.svelte";
  import UpdatesLoadingCard from "$components/features/updates/UpdatesLoadingCard.svelte";
  import { locale, t } from "$lib/i18n";
  import type { UpdateFeedItem, UpdatesSection } from "$lib/features/updates/types";
  import { selectItems } from "$lib/query-selectors";
  import { QUERY_CACHE_TIMES, visibleRefetchInterval } from "$lib/query-client";
  import { dehydratedQueryOptions } from "$lib/query-hydration";
  import { getErrorMessage, parseDateValue } from "$lib/utils";
  import { Library as LibraryIcon, RefreshCw } from "@lucide/svelte";
  import { untrack } from "svelte";
  import { useHydrate, useQueryClient, type DehydratedState } from "@tanstack/svelte-query";

  import type { LibraryChapter, LibraryManga } from "$lib/types";

  const { data } = $props<{
    data: {
      dehydratedState?: DehydratedState;
    };
  }>();
  const queryClient = useQueryClient();

  useHydrate(untrack(() => data.dehydratedState), undefined, queryClient);

  const EMPTY_LIBRARY: LibraryManga[] = [];
  const EMPTY_UPDATES: LibraryChapter[] = [];

  const libraryQuery = createGetLibrary(() => ({
    query: {
      ...dehydratedQueryOptions(data.dehydratedState, getGetLibraryQueryKey()),
      select: selectItems<LibraryManga>,
      staleTime: QUERY_CACHE_TIMES.stable,
    },
  }));

  const updatesQuery = createGetLibraryUpdates(() => ({
    query: {
      ...dehydratedQueryOptions(data.dehydratedState, getGetLibraryUpdatesQueryKey()),
      refetchInterval: visibleRefetchInterval(60_000),
      select: selectItems<LibraryChapter>,
      staleTime: QUERY_CACHE_TIMES.live,
    },
  }));

  const refreshMutation = createTriggerLibraryUpdate();

  const library = $derived(libraryQuery.data ?? EMPTY_LIBRARY);
  const updates = $derived(updatesQuery.data ?? EMPTY_UPDATES);
  const pageLoading = $derived(updates.length === 0 && updatesQuery.isPending);
  const feedLoading = $derived(updates.length > 0 && libraryQuery.isPending && library.length === 0);
  const error = $derived(
    updatesQuery.error
      ? getErrorMessage(updatesQuery.error, $t("app.updates.requestFailed"))
      : (libraryQuery.error
        ? getErrorMessage(libraryQuery.error, $t("app.library.requestFailed"))
        : "")
  );

  let refreshing = $state(false);
  let refreshMessage = $state("");

  const sectionFormatter = $derived(new Intl.DateTimeFormat($locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }));

  const timeFormatter = $derived(new Intl.DateTimeFormat($locale, {
    hour: "numeric",
    minute: "2-digit",
  }));

  const dateFormatter = $derived(new Intl.DateTimeFormat($locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }));

  function errorMessage(error: unknown) {
    return getErrorMessage(error, $t("app.errors.genericShort"));
  }

  function sectionLabel(date: Date | null): string {
    if (!date) {
      return $t("app.library.unknown");
    }

    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const target = new Date(date.getFullYear(), date.getMonth(), date.getDate());
    const diffDays = Math.round((today.getTime() - target.getTime()) / 86_400_000);

    if (diffDays === 0) {
      return $t("app.dates.today");
    }
    if (diffDays === 1) {
      return $t("app.dates.yesterday");
    }

    return sectionFormatter.format(date);
  }

  function isWithinLastWeek(date: Date | null): boolean {
    if (!date) {
      return false;
    }

    const now = new Date();
    const diffMs = now.getTime() - date.getTime();

    return diffMs >= 0 && diffMs < 7 * 86_400_000;
  }

  function visibleCategoryLabel(item: UpdateFeedItem): string {
    const category = (item.manga.category ?? "").trim();
    if (!category) {
      return "";
    }

    const statusLabel = item.chapter.downloaded ? "downloaded" : "new";
    return category.toLocaleLowerCase() === statusLabel ? "" : category;
  }

  const libraryById = $derived(getLibraryById(library));
  const recentFeed = $derived(getRecentFeed(updates, libraryById));
  const recentFeedItems = $derived(recentFeed.items);
  const sections = $derived(recentFeed.sections);

  const lastUpdateLabel = $derived(
    recentFeedItems.length > 0
      ? $t("app.updates.last7Days")
      : $t("app.updates.emptyTitle")
  );

  function getLibraryById(libraryItems: LibraryManga[]) {
    const mangasById: Record<string, LibraryManga> = {};
    for (const manga of libraryItems) {
      mangasById[manga.id] = manga;
    }

    return mangasById;
  }

  function getRecentFeed(
    updateItems: LibraryChapter[],
    mangasById: Record<string, LibraryManga>,
  ): { items: UpdateFeedItem[]; sections: UpdatesSection[] } {
    const items: UpdateFeedItem[] = [];

    for (const chapter of updateItems) {
      const manga = mangasById[chapter.manga_id];
      if (!manga) {
        continue;
      }

      const releaseAt = parseDateValue(chapter.date_uploaded);
      if (!isWithinLastWeek(releaseAt)) {
        continue;
      }

      const pickedUpAt = parseDateValue(chapter.fetched_at);
      items.push({
        chapter,
        manga,
        pickedUpAt,
        pickedUpTimeLabel: pickedUpAt ? timeFormatter.format(pickedUpAt) : $t("app.library.unknown"),
        releaseAt,
        releaseDateLabel: releaseAt ? dateFormatter.format(releaseAt) : $t("app.library.unknown"),
        releaseTimeLabel: releaseAt ? timeFormatter.format(releaseAt) : $t("app.library.unknown"),
      });
    }

    items.sort((left: UpdateFeedItem, right: UpdateFeedItem) => {
      const leftTime = left.releaseAt?.getTime() ?? 0;
      const rightTime = right.releaseAt?.getTime() ?? 0;
      return rightTime - leftTime;
    });

    const buckets: Record<string, UpdateFeedItem[]> = {};
    const sections: UpdatesSection[] = [];

    for (const item of items) {
      const label = sectionLabel(item.releaseAt);
      let bucket = buckets[label];

      if (!bucket) {
        bucket = [];
        buckets[label] = bucket;
        sections.push({ items: bucket, label });
      }

      bucket.push(item);
    }

    return { items, sections };
  }

  async function refreshUpdates() {
    refreshing = true;
    refreshMessage = "";

    try {
      const result = await refreshMutation.mutateAsync();
      refreshMessage = result.new_chapters === 0
        ? $t("app.updates.noNewChapters")
        : $t("app.updates.pickedUpCount", { count: result.new_chapters });

      await Promise.all([libraryQuery.refetch(), updatesQuery.refetch()]);
    } catch (queryError: unknown) {
      refreshMessage = errorMessage(queryError);
    } finally {
      refreshing = false;
    }
  }
</script>

<svelte:head>
  <title>{$t("app.updates.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<div class="space-y-6">
  <PageHeader
    title={$t("app.updates.title")}
    description={$t("app.updates.description")}
    actionsClass="pt-2 lg:pt-4"
  >
    {#snippet actions()}
      <div class="flex min-w-[14rem] flex-col gap-1.5 sm:items-end">
        <Button
          variant="secondary"
          class="h-9"
          onclick={refreshUpdates}
          disabled={refreshing}
        >
          <RefreshCw class={refreshing ? "mr-2 h-4 w-4 animate-spin" : "mr-2 h-4 w-4"} />
          {refreshing ? $t("app.updates.checking") : $t("app.updates.checkNow")}
        </Button>
        <p class="text-xs text-muted-foreground">{lastUpdateLabel}</p>
        {#if refreshMessage}
          <p class="text-sm text-muted-foreground">{refreshMessage}</p>
        {/if}
      </div>
    {/snippet}
  </PageHeader>

  {#if pageLoading || feedLoading}
    <UpdatesLoadingCard />
  {:else if error}
    <Alert variant="destructive">
      <AlertTitle>{$t("app.updates.unavailable")}</AlertTitle>
      <AlertDescription>
        <p>{$t("app.library.serverPortHint")}</p>
        <p>{error}</p>
      </AlertDescription>
    </Alert>
  {:else if sections.length === 0}
    <EmptyState
      icon={LibraryIcon}
      title={$t("app.updates.emptyTitle")}
      description={$t("app.updates.emptyDescription")}
    />
  {:else}
    <UpdatesFeedCard
      {sections}
      itemCount={recentFeedItems.length}
      getCategoryLabel={visibleCategoryLabel}
    />
  {/if}
</div>
