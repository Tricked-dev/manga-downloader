<script lang="ts">
  import {
    createPauseUpscaling,
    createResumeUpscaling,
    createUpscaleDownload,
    createUpscaleQueue,
    getUpscaleQueueQueryKey,
  } from "@manga-server/api-client/generated";
  import type { UpscaleQueueEntry, UpscaleQueueResponse } from "@manga-server/api-client/generated";
  import { useQueryClient } from "@tanstack/svelte-query";
  import { SvelteSet } from "svelte/reactivity";
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { Progress } from "$lib/ui/progress";
  import SettingsNotice from "$components/features/settings/SettingsNotice.svelte";
  import {
    countUpscaleStatus,
    sortUpscaleQueue,
    upscalePercent,
    upscaleStatusVariant,
  } from "$lib/features/downloads/upscale-queue";
  import { t } from "$lib/i18n";
  import { QUERY_CACHE_TIMES, visibleRefetchInterval } from "$lib/query-client";
  import { getErrorMessage } from "$lib/utils";
  import { Loader2, Pause, Play, RefreshCw, Sparkles } from "@lucide/svelte";

  const EMPTY_ENTRIES: UpscaleQueueEntry[] = [];

  const queryClient = useQueryClient();

  let queueQuery: ReturnType<typeof createUpscaleQueue<UpscaleQueueResponse>>;

  // A paused queue changes only when someone acts on it, so only live work is worth polling.
  function shouldPollQueue() {
    const current = queueQuery?.data;
    return Boolean(current && !current.paused && current.items.length > 0);
  }

  queueQuery = createUpscaleQueue<UpscaleQueueResponse>(() => ({
    query: {
      refetchInterval: visibleRefetchInterval(3000, shouldPollQueue),
      staleTime: QUERY_CACHE_TIMES.live,
    },
  }));

  const pauseMutation = createPauseUpscaling();
  const resumeMutation = createResumeUpscaling();
  const retryMutation = createUpscaleDownload();

  let errorMessage = $state("");
  const retryingIds = new SvelteSet<string>();

  const queue = $derived(queueQuery.data);
  const entries = $derived(sortUpscaleQueue(queue?.items ?? EMPTY_ENTRIES));
  const paused = $derived(queue?.paused ?? false);
  const autoResumeMinutes = $derived(queue?.auto_resume_minutes ?? 0);
  const runningCount = $derived(countUpscaleStatus(entries, "running"));
  const queuedCount = $derived(countUpscaleStatus(entries, "queued"));
  const failedCount = $derived(countUpscaleStatus(entries, "failed"));
  const busy = $derived(pauseMutation.isPending || resumeMutation.isPending);

  async function refreshQueue() {
    await queryClient.invalidateQueries({ queryKey: getUpscaleQueueQueryKey() });
  }

  async function setPaused(next: boolean) {
    errorMessage = "";
    try {
      await (next ? pauseMutation : resumeMutation).mutateAsync();
      await refreshQueue();
    } catch (cause) {
      errorMessage = getErrorMessage(cause);
    }
  }

  async function retryUpscale(downloadId: string) {
    errorMessage = "";
    retryingIds.add(downloadId);
    try {
      await retryMutation.mutateAsync({ id: downloadId });
      await refreshQueue();
    } catch (cause) {
      errorMessage = getErrorMessage(cause);
    } finally {
      retryingIds.delete(downloadId);
    }
  }
</script>

{#if queue && (entries.length > 0 || paused)}
  <Card class="gap-0">
    <CardHeader class="border-b border-border/70 py-4">
      <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <CardTitle class="flex items-center gap-2">
            <Sparkles class="h-4 w-4 text-primary" />
            {$t("app.downloads.upscale.title")}
          </CardTitle>
          <CardDescription>
            {#if paused && autoResumeMinutes > 0}
              {$t("app.downloads.upscale.pausedAutoResume", { count: autoResumeMinutes })}
            {:else if paused}
              {$t("app.downloads.upscale.pausedDescription")}
            {:else}
              {$t("app.downloads.upscale.runningDescription")}
            {/if}
          </CardDescription>
        </div>
        <div class="flex flex-wrap items-center gap-2">
          <Badge variant={paused ? "warning" : "info"} class="h-7 px-2.5">
            {paused ? $t("app.settings.upscaleQueuePaused") : $t("app.settings.upscaleQueueRunning")}
          </Badge>
          <Badge variant="outline" class="h-7 px-2.5">
            {$t("app.downloads.upscale.counts", {
              running: runningCount,
              queued: queuedCount,
              failed: failedCount,
            })}
          </Badge>
          <Button variant="outline" size="sm" disabled={busy} onclick={() => setPaused(!paused)}>
            {#if busy}
              <Loader2 class="mr-1.5 h-3.5 w-3.5 animate-spin" />
            {:else if paused}
              <Play class="mr-1.5 h-3.5 w-3.5" />
            {:else}
              <Pause class="mr-1.5 h-3.5 w-3.5" />
            {/if}
            {paused ? $t("app.settings.upscaleQueueResume") : $t("app.settings.upscaleQueuePause")}
          </Button>
        </div>
      </div>
    </CardHeader>
    <CardContent class="p-0">
      {#if errorMessage}
        <div class="px-4 pt-4">
          <SettingsNotice>{errorMessage}</SettingsNotice>
        </div>
      {/if}
      {#if entries.length === 0}
        <p class="px-4 py-6 text-sm text-muted-foreground">
          {$t("app.downloads.upscale.empty")}
        </p>
      {:else}
        <ul class="divide-y divide-border/70">
          {#each entries as entry (entry.download_id)}
            <li class="grid gap-3 px-4 py-3 md:grid-cols-[minmax(0,2.2fr)_minmax(0,1fr)_110px] md:items-center">
              <div class="min-w-0 space-y-1">
                <p class="truncate text-sm font-medium">
                  {entry.manga_title}
                  <span class="text-muted-foreground">
                    · {$t("app.downloads.upscale.chapter", { number: entry.chapter_number })}
                  </span>
                </p>
                <p class="truncate text-xs text-muted-foreground">{entry.message}</p>
              </div>
              <div class="space-y-1.5">
                <div class="flex items-center gap-2">
                  <Badge variant={upscaleStatusVariant(entry.status)}>{entry.status}</Badge>
                  {#if entry.total_pages > 0}
                    <span class="text-xs text-muted-foreground">
                      {entry.completed_pages}/{entry.total_pages}
                    </span>
                  {/if}
                </div>
                {#if entry.total_pages > 0}
                  <Progress value={upscalePercent(entry)} max={100} />
                {/if}
              </div>
              <div class="md:justify-self-end">
                {#if entry.status === "failed"}
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={retryingIds.has(entry.download_id)}
                    onclick={() => retryUpscale(entry.download_id)}
                  >
                    {#if retryingIds.has(entry.download_id)}
                      <Loader2 class="mr-1.5 h-3.5 w-3.5 animate-spin" />
                    {:else}
                      <RefreshCw class="mr-1.5 h-3.5 w-3.5" />
                    {/if}
                    {$t("app.actions.retry")}
                  </Button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </CardContent>
  </Card>
{/if}
