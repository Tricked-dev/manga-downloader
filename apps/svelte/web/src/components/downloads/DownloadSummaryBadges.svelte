<script lang="ts">
  import { AlertTriangle, CheckCircle2, Clock3, Download } from "@lucide/svelte";
  import { Badge } from "$lib/ui/badge";
  import { isActiveDownloadStatus } from "$lib/features/downloads/status";
  import { t } from "$lib/i18n";
  import type { DownloadItem } from "$lib/types";

  const { items = [], showCompleted = true } = $props<{ items?: DownloadItem[]; showCompleted?: boolean }>();

  const completedCount = $derived(countCompleted());
  const downloadingCount = $derived(countDownloading());
  const errorCount = $derived(countError());
  const queuedCount = $derived(countQueued());
  const activeProgress = $derived(firstActiveProgress());

  const hasVisibleSummary = $derived(
    downloadingCount > 0 || queuedCount > 0 || errorCount > 0 || (showCompleted && completedCount > 0),
  );

  function countCompleted(): number {
    let count = 0;
    for (const item of items) {
      if (item.status === "completed") {
        count += 1;
      }
    }

    return count;
  }

  function countDownloading(): number {
    let count = 0;
    for (const item of items) {
      if (isActiveDownloadStatus(item.status)) {
        count += 1;
      }
    }

    return count;
  }

  function countError(): number {
    let count = 0;
    for (const item of items) {
      if (item.status === "error") {
        count += 1;
      }
    }

    return count;
  }

  function countQueued(): number {
    let count = 0;
    for (const item of items) {
      if (item.status === "queued") {
        count += 1;
      }
    }

    return count;
  }

  function firstActiveProgress(): number | null {
    for (const item of items) {
      if (isActiveDownloadStatus(item.status)) {
        return Math.round(item.progress);
      }
    }

    return null;
  }
</script>

{#if hasVisibleSummary}
  <div class="flex flex-wrap gap-1.5">
    {#if downloadingCount > 0}
      <Badge variant="info" class="gap-1 text-[10px]">
        <Download class="h-3 w-3" />
        {activeProgress !== null
          ? $t("app.downloads.labels.activeWithProgress", { count: downloadingCount, progress: activeProgress })
          : $t("app.downloads.labels.active", { count: downloadingCount })}
      </Badge>
    {/if}

    {#if queuedCount > 0}
      <Badge variant="warning" class="gap-1 text-[10px]">
        <Clock3 class="h-3 w-3" />
        {$t("app.downloads.labels.queued", { count: queuedCount })}
      </Badge>
    {/if}

    {#if errorCount > 0}
      <Badge variant="danger" class="gap-1 text-[10px]">
        <AlertTriangle class="h-3 w-3" />
        {$t("app.downloads.labels.failed", { count: errorCount })}
      </Badge>
    {/if}

    {#if showCompleted && completedCount > 0}
      <Badge variant="success" class="gap-1 text-[10px]">
        <CheckCircle2 class="h-3 w-3" />
        {$t("app.downloads.done", { count: completedCount })}
      </Badge>
    {/if}
  </div>
{/if}
