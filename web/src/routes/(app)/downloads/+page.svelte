<script lang="ts">
  import {
    createClearFailedDownloads,
    createDeleteDownload,
    createGetDownloads,
  } from "@manga-server/api-client/generated";
  import {
    clearFailedDownloads as clearFailedDownloadCommands,
    removeDownloads as removeDownloadCommands,
  } from "$lib/features/downloads/download-commands";
  import { getDownloadStatusCounts, isActiveDownloadStatus } from "$lib/features/downloads/status";
  import { useQueryClient } from "@tanstack/svelte-query";
  import { SvelteSet } from "svelte/reactivity";
  import { Button } from "$lib/ui/button";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { Skeleton } from "$lib/ui/skeleton";
  import PageHeader from "@manga-server/ui/components/page-header";
  import EmptyState from "$components/common/EmptyState.svelte";
  import DownloadsSummary from "$components/features/downloads/DownloadsSummary.svelte";
  import DownloadsTable from "$components/features/downloads/DownloadsTable.svelte";
  import UpscaleQueuePanel from "$components/features/downloads/UpscaleQueuePanel.svelte";
  import { t } from "$lib/i18n";
  import { selectItems } from "$lib/query-selectors";
  import { QUERY_CACHE_TIMES, visibleRefetchInterval } from "$lib/query-client";
  import type { DownloadItem } from "$lib/types";
  import { Download, Trash2 } from "@lucide/svelte";

  const EMPTY_DOWNLOADS: DownloadItem[] = [];

  const queryClient = useQueryClient();
  let downloadsQuery: ReturnType<typeof createGetDownloads<DownloadItem[]>>;

  function shouldPollDownloads() {
    const currentDownloads = downloadsQuery?.data ?? EMPTY_DOWNLOADS;
    return currentDownloads.some((download: DownloadItem) => isActiveDownloadStatus(download.status));
  }

  downloadsQuery = createGetDownloads<DownloadItem[]>(undefined, () => ({
    query: {
      refetchInterval: visibleRefetchInterval(3000, shouldPollDownloads),
      select: selectItems<DownloadItem>,
      staleTime: QUERY_CACHE_TIMES.live,
    },
  }));

  const clearFailedMutation = createClearFailedDownloads();

  const removeDownloadMutation = createDeleteDownload();

  const downloads: DownloadItem[] = $derived(downloadsQuery.data ?? EMPTY_DOWNLOADS);
  const loading = $derived(downloadsQuery.isPending);
  const downloadStatusCounts = $derived(getDownloadStatusCounts(downloads));
  const activeDownloadCount = $derived(downloadStatusCounts.active);
  const queuedDownloadCount = $derived(downloadStatusCounts.queued);
  const completedDownloadCount = $derived(downloadStatusCounts.completed);
  const failedDownloadCount = $derived(downloadStatusCounts.failed);
  const clearingFailed = $derived(clearFailedMutation.isPending);
  const downloadSkeletonRows = Array.from({ length: 5 }, (_, index) => index);
  const removingDownloadIds = new SvelteSet<string>();

  async function clearFailedDownloads() {
    await clearFailedDownloadCommands(clearFailedMutation, queryClient);
  }

  async function removeDownload(id: string) {
    await removeDownloads([id]);
  }

  async function removeDownloads(ids: string[]) {
    await removeDownloadCommands(removeDownloadMutation, queryClient, ids, removingDownloadIds);
  }
</script>

<svelte:head>
  <title>{$t("app.downloads.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<div class="space-y-5">
  <PageHeader title={$t("app.downloads.title")} description={$t("app.downloads.description")}>
    {#snippet actions()}
      <div class="flex flex-col gap-3 sm:items-end">
        {#if loading}
          <div class="flex flex-wrap gap-2">
            {#each Array.from({ length: 5 }, (_, index) => index) as index (index)}
              <Skeleton class="h-9 w-24 rounded-lg" />
            {/each}
          </div>
          <Skeleton class="h-9 w-28 rounded-md" />
        {:else}
          <DownloadsSummary
            total={downloads.length}
            active={activeDownloadCount}
            queued={queuedDownloadCount}
            completed={completedDownloadCount}
            failed={failedDownloadCount}
          />
          <div class="flex flex-wrap gap-2">
            <Button
              variant="outline"
              size="sm"
              onclick={clearFailedDownloads}
              disabled={failedDownloadCount === 0 || clearingFailed}
            >
              <Trash2 class="mr-1.5 h-3.5 w-3.5" />
              {clearingFailed ? $t("app.actions.clearing") : $t("app.actions.clearFailed")}
            </Button>
          </div>
        {/if}
      </div>
    {/snippet}
  </PageHeader>

  <UpscaleQueuePanel />

  {#if loading}
    <Card class="gap-0">
      <CardHeader class="border-b border-border/70 py-4">
        <div class="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <CardTitle>{$t("app.downloads.queue")}</CardTitle>
            <CardDescription>{$t("app.downloads.queueDescription")}</CardDescription>
          </div>
          <Skeleton class="h-4 w-28" />
        </div>
      </CardHeader>
      <CardContent class="p-0">
        <div class="divide-y divide-border/70">
          {#each downloadSkeletonRows as index (index)}
            <div class="grid gap-4 px-4 py-4 md:grid-cols-[minmax(0,2.4fr)_minmax(0,1.2fr)_minmax(0,0.9fr)_minmax(0,1fr)_120px] md:items-center">
              <div class="space-y-2">
                <Skeleton class="h-4 w-3/4" />
                <Skeleton class="h-3 w-1/3" />
              </div>
              <Skeleton class="h-4 w-24" />
              <Skeleton class="h-6 w-20 rounded-md" />
              <div class="space-y-2">
                <Skeleton class="h-2 w-full rounded-full" />
                <Skeleton class="h-3 w-12" />
              </div>
              <Skeleton class="h-8 w-24 rounded-md md:justify-self-end" />
            </div>
          {/each}
        </div>
      </CardContent>
    </Card>
  {:else if downloads.length === 0}
    <EmptyState
      icon={Download}
      title={$t("app.downloads.emptyTitle")}
      description={$t("app.downloads.emptyDescription")}
    />
  {:else}
    <Card class="gap-0">
      <CardHeader class="border-b border-border/70 py-4">
        <div class="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <CardTitle>{$t("app.downloads.queue")}</CardTitle>
            <CardDescription>{$t("app.downloads.queueDescription")}</CardDescription>
          </div>
          <p class="text-xs text-muted-foreground">
            {#if queuedDownloadCount > 0}
              {$t("app.downloads.queuedNext", { count: queuedDownloadCount })}
            {:else}
              {$t("app.downloads.queueClear")}
            {/if}
          </p>
        </div>
      </CardHeader>
      <CardContent class="p-0">
        <DownloadsTable
          {downloads}
          removingIds={removingDownloadIds}
          onRemove={removeDownload}
          onBulkRemove={removeDownloads}
        />
      </CardContent>
    </Card>
  {/if}
</div>
