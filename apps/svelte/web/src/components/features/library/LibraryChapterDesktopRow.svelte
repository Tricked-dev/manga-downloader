<script lang="ts">
  import ChapterDownloadStatusCell from "$components/features/library/ChapterDownloadStatusCell.svelte";
  import { ACTIVE_DOWNLOAD_STATUSES, getTranslatedDownloadDisplayLabel } from "$lib/features/downloads/status";
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";
  import { Progress } from "$lib/ui/progress";
  import { TableCell, TableRow } from "$lib/ui/table";
  import { t } from "$lib/i18n";
  import { formatChapterNumber, formatDateLabel } from "$lib/utils";
  import { Check } from "@lucide/svelte";

  let {
    chapterTableRow,
    mangaSource,
    row,
    chapterReadLabel,
    publicView = false,
    selectionBoxClass,
  } = $props<{
    chapterTableRow: any;
    mangaSource: string;
    row: any;
    chapterReadLabel: (chapter: any) => string;
    publicView?: boolean;
    selectionBoxClass: (selected: boolean) => string;
  }>();

  const chapter = $derived(chapterTableRow.chapter);
  const chapterDownload = $derived(chapterTableRow.download);
</script>

<TableRow data-state={row.getIsSelected() ? "selected" : undefined} class="group">
  {#each row.getAllCells() as cell (cell.id)}
    {#if cell.column.id === "select"}
      <TableCell class={publicView ? "hidden" : "pl-5"}>
        {#if !publicView}
          <label class="group inline-flex size-7 cursor-pointer items-center justify-center focus-within:ring-2 focus-within:ring-ring">
            <input
              type="checkbox"
              class="sr-only"
              checked={row.getIsSelected()}
              aria-label={$t("app.libraryDetail.selectChapter", { number: formatChapterNumber(chapter.chapter_number) })}
              onchange={row.getToggleSelectedHandler()}
            />
            <span class={selectionBoxClass(row.getIsSelected())}>
              {#if row.getIsSelected()}
                <Check class="size-3" />
              {/if}
            </span>
          </label>
        {/if}
      </TableCell>
    {:else if cell.column.id === "chapter"}
      <TableCell class="pl-5">
        <p class="text-sm font-medium">{$t("app.downloads.chapterWithNumber", { number: formatChapterNumber(chapter.chapter_number) })}</p>
      </TableCell>
    {:else if cell.column.id === "uploaded"}
      <TableCell>
        {#if chapter.date_uploaded}
          <Badge variant="secondary">{formatDateLabel(chapter.date_uploaded)}</Badge>
        {:else}
          <span class="text-xs text-muted-foreground">{$t("app.library.unknown")}</span>
        {/if}
      </TableCell>
    {:else if cell.column.id === "status"}
      <TableCell>
        <ChapterDownloadStatusCell download={chapterDownload} chapterDownloaded={chapter.downloaded} />
      </TableCell>
    {:else if cell.column.id === "read"}
      <TableCell>
        <Badge variant={chapter.read_completed ? "success" : chapter.pages_read > 0 ? "secondary" : "outline"}>
          {chapterReadLabel(chapter)}
        </Badge>
      </TableCell>
    {:else if cell.column.id === "progress"}
      <TableCell>
        {#if chapterDownload && ACTIVE_DOWNLOAD_STATUSES.has(chapterDownload.status)}
          <div class="min-w-32 max-w-44 space-y-1">
            <Progress value={Math.round(chapterDownload.progress)} class="h-2" />
            <p class="text-xs text-muted-foreground">
              {getTranslatedDownloadDisplayLabel(chapterDownload.status, $t)} {Math.round(chapterDownload.progress)}%
            </p>
          </div>
        {:else if chapterDownload}
          <span class="text-xs text-muted-foreground">{Math.round(chapterDownload.progress)}%</span>
        {:else if chapter.downloaded}
          <span class="text-xs text-muted-foreground">100%</span>
        {:else}
          <span class="text-xs text-muted-foreground">-</span>
        {/if}
      </TableCell>
    {:else if cell.column.id === "actions"}
      <TableCell class="pr-5 text-right">
        <div class="flex justify-end">
          <Button
            href={`/read/${encodeURIComponent(mangaSource)}/${encodeURIComponent(chapter.source_id)}?chapterId=${encodeURIComponent(chapter.id)}&libraryId=${encodeURIComponent(chapter.manga_id)}`}
            variant="secondary"
            size="sm"
          >
            {$t("app.manga.read")}
          </Button>
        </div>
      </TableCell>
    {/if}
  {/each}
</TableRow>
