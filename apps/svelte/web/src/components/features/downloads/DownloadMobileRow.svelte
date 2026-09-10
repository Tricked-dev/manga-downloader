<script lang="ts">
  import type { Row } from "@tanstack/svelte-table";
  import { Check } from "@lucide/svelte";
  import { Progress } from "$lib/ui/progress";
  import DownloadStatusBadge from "$components/downloads/DownloadStatusBadge.svelte";
  import DownloadActionButton from "$components/features/downloads/DownloadActionButton.svelte";
  import { isActiveDownloadStatus } from "$lib/features/downloads/status";
  import { t } from "$lib/i18n";
  import { formatChapterNumber } from "$lib/utils";
  import type { DownloadItem } from "$lib/types";

  const {
    download,
    onRemove,
    removingIds,
    row,
    selectionBoxClass,
  }: {
    download: DownloadItem;
    onRemove: (id: string) => void;
    removingIds: Set<string>;
    row: Row<any, DownloadItem>;
    selectionBoxClass: (selected: boolean) => string;
  } = $props();

  function chapterLabel(downloadItem: DownloadItem): string {
    return $t("app.downloads.chapterWithNumber", { number: formatChapterNumber(downloadItem.chapter_number) });
  }

  function chapterMeta(downloadItem: DownloadItem): string {
    const title = downloadItem.chapter_title.trim();
    if (title && title.toLocaleLowerCase() !== chapterLabel(downloadItem).toLocaleLowerCase()) {
      return title;
    }

    return $t("app.downloads.id", { id: compactChapterId(downloadItem.chapter_source_id || downloadItem.chapter_id) });
  }

  function chapterMetaTitle(downloadItem: DownloadItem): string {
    const title = downloadItem.chapter_title.trim();
    if (title && title.toLocaleLowerCase() !== chapterLabel(downloadItem).toLocaleLowerCase()) {
      return title;
    }

    return `ID: ${downloadItem.chapter_source_id || downloadItem.chapter_id}`;
  }

  function compactChapterId(id: string): string {
    const trimmedId = id.trim();

    try {
      const url = new URL(trimmedId);
      const lastPathSegment = url.pathname.split("/").filter(Boolean).at(-1);
      if (lastPathSegment) {
        return lastPathSegment;
      }
    } catch {
      // Plain source IDs are expected for most providers.
    }

    if (trimmedId.length <= 36) {
      return trimmedId;
    }

    return `${trimmedId.slice(0, 16)}...${trimmedId.slice(-12)}`;
  }
</script>

<article data-state={row.getIsSelected() ? "selected" : undefined} class="space-y-3 bg-card p-4 data-[state=selected]:bg-muted">
  <div class="flex items-start gap-3">
    <label class="group mt-0.5 inline-flex size-8 cursor-pointer items-center justify-center focus-within:ring-2 focus-within:ring-ring">
      <input
        type="checkbox"
        class="sr-only"
        checked={row.getIsSelected()}
        aria-label={$t("app.downloads.select", { title: download.manga_title, chapter: chapterLabel(download) })}
        onchange={row.getToggleSelectedHandler()}
      />
      <span class={selectionBoxClass(row.getIsSelected())}>
        {#if row.getIsSelected()}
          <Check class="size-3" />
        {/if}
      </span>
    </label>

    <div class="min-w-0 flex-1 space-y-1">
      <p class="line-clamp-2 text-sm font-medium">{download.manga_title}</p>
      <p class="truncate text-xs text-muted-foreground">{download.manga_source}</p>
    </div>

    <DownloadStatusBadge status={download.status} />
  </div>

  <div class="space-y-1 pl-11">
    <p class="text-sm">{chapterLabel(download)}</p>
    <p class="break-all text-xs text-muted-foreground" title={chapterMetaTitle(download)}>
      {chapterMeta(download)}
    </p>
  </div>

  <div class="flex flex-col gap-3 pl-11 sm:flex-row sm:items-center sm:justify-between">
    <div class="min-w-0 flex-1">
      {#if isActiveDownloadStatus(download.status)}
        <Progress value={Math.round(download.progress)} class="h-2" />
        <p class="mt-1 text-xs text-muted-foreground">{Math.round(download.progress)}%</p>
      {:else}
        <p class="text-xs text-muted-foreground">{$t("app.downloads.progressComplete", { progress: Math.round(download.progress) })}</p>
      {/if}
    </div>

    <DownloadActionButton
      {download}
      {removingIds}
      {onRemove}
      class="w-full sm:w-auto"
    />
  </div>
</article>
