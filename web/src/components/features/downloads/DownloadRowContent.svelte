<script lang="ts">
  import type { Row } from "@tanstack/svelte-table";
  import { Check } from "@lucide/svelte";
  import { Progress } from "$lib/ui/progress";
  import { TableCell, TableRow } from "$lib/ui/table";
  import UpscaleStatus from "$components/downloads/UpscaleStatus.svelte";
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

<TableRow data-state={row.getIsSelected() ? "selected" : undefined} class="group">
  {#each row.getAllCells() as cell (cell.id)}
    {#if cell.column.id === "select"}
      <TableCell class="pl-4">
        <label class="group inline-flex size-7 cursor-pointer items-center justify-center focus-within:ring-2 focus-within:ring-ring">
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
      </TableCell>
    {:else if cell.column.id === "series"}
      <TableCell>
        <div class="min-w-0">
          <p class="truncate text-sm font-medium">{download.manga_title}</p>
          <p class="truncate text-xs text-muted-foreground">{download.manga_source}</p>
        </div>
      </TableCell>
    {:else if cell.column.id === "chapter"}
      <TableCell>
        <div class="min-w-0 max-w-[18rem]">
          <p class="truncate text-sm">{chapterLabel(download)}</p>
          <p class="truncate text-xs text-muted-foreground" title={chapterMetaTitle(download)}>
            {chapterMeta(download)}
          </p>
        </div>
      </TableCell>
    {:else if cell.column.id === "status"}
      <TableCell>
        <DownloadStatusBadge status={download.status} />
    <UpscaleStatus {download} />
      </TableCell>
    {:else if cell.column.id === "progress"}
      <TableCell>
        {#if isActiveDownloadStatus(download.status)}
          <div class="min-w-28">
            <Progress value={Math.round(download.progress)} class="h-2" />
            <p class="mt-1 text-xs text-muted-foreground">{Math.round(download.progress)}%</p>
          </div>
        {:else}
          <p class="text-xs text-muted-foreground">{Math.round(download.progress)}%</p>
        {/if}
      </TableCell>
    {:else if cell.column.id === "actions"}
      <TableCell class="pr-4 text-right">
        <DownloadActionButton
          {download}
          {removingIds}
          {onRemove}
          stopPropagation
        />
      </TableCell>
    {/if}
  {/each}
</TableRow>
