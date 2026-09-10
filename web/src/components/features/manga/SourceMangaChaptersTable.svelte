<script lang="ts">
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";
  import { TableCell, TableRow } from "$lib/ui/table";
  import SourceChapterDownloadButton from "$components/features/manga/SourceChapterDownloadButton.svelte";
  import SourceChapterStatusBadge from "$components/features/manga/SourceChapterStatusBadge.svelte";
  import SourceMangaChaptersCard from "$components/features/manga/SourceMangaChaptersCard.svelte";
  import TableSortHeader from "$components/common/TableSortHeader.svelte";
  import VirtualizedTanStackTable from "$components/common/VirtualizedTanStackTable.svelte";
  import { t } from "$lib/i18n";
  import { formatChapterNumber, formatDateLabel } from "$lib/utils";

  let {
    actionMessage,
    chapterColumns,
    chapterHeaderCellClass,
    chapterTable,
    chaptersError,
    chaptersLoading,
    count,
    onDownloadChapter,
    sortLabel,
    sortedChapterRows,
    sourceName,
  } = $props<any>();
</script>

{#snippet chapterHeader(header: any)}
  <TableSortHeader
    label={sortLabel(header.column.columnDef.header)}
    canSort={header.column.getCanSort()}
    sorted={header.column.getIsSorted()}
    onclick={header.column.getToggleSortingHandler()}
    class={header.column.id === "actions" ? "ml-auto" : ""}
  />
{/snippet}

{#snippet chapterRow(row: any, chapterTableRow: any)}
  {@const chapter = chapterTableRow.chapter}
  {@const queued = chapterTableRow.queued}
  {@const downloading = chapterTableRow.downloading}
  <TableRow>
    {#each row.getAllCells() as cell (cell.id)}
      {#if cell.column.id === "chapter"}
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
          <SourceChapterStatusBadge {downloading} {queued} />
        </TableCell>
      {:else if cell.column.id === "actions"}
        <TableCell class="pr-5 text-right">
          <div class="flex justify-end gap-2">
            <Button
              href={`/read/${encodeURIComponent(sourceName)}/${encodeURIComponent(chapter.id)}`}
              variant="secondary"
              size="sm"
            >
              {$t("app.manga.read")}
            </Button>
            <SourceChapterDownloadButton
              chapterId={chapter.id}
              {downloading}
              {queued}
              onDownload={onDownloadChapter}
            />
          </div>
        </TableCell>
      {/if}
    {/each}
  </TableRow>
{/snippet}

{#snippet chapterMobileRow(_row: any, chapterTableRow: any)}
  {@const chapter = chapterTableRow.chapter}
  {@const queued = chapterTableRow.queued}
  {@const downloading = chapterTableRow.downloading}
  <article class="space-y-3 bg-card p-4">
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <p class="text-sm font-medium">{$t("app.downloads.chapterWithNumber", { number: formatChapterNumber(chapter.chapter_number) })}</p>
        {#if chapter.date_uploaded}
          <p class="text-xs text-muted-foreground">{formatDateLabel(chapter.date_uploaded)}</p>
        {:else}
          <p class="text-xs text-muted-foreground">{$t("app.manga.unknownUploadDate")}</p>
        {/if}
      </div>

      <SourceChapterStatusBadge {downloading} {queued} />
    </div>

    <div class="grid grid-cols-2 gap-2">
      <Button
        href={`/read/${encodeURIComponent(sourceName)}/${encodeURIComponent(chapter.id)}`}
        variant="secondary"
        size="sm"
      >
        {$t("app.manga.read")}
      </Button>
      <SourceChapterDownloadButton
        chapterId={chapter.id}
        {downloading}
        {queued}
        onDownload={onDownloadChapter}
      />
    </div>
  </article>
{/snippet}

<SourceMangaChaptersCard
  {actionMessage}
  {chaptersError}
  {chaptersLoading}
  {count}
  {sourceName}
>
  <VirtualizedTanStackTable
    table={chapterTable}
    rows={sortedChapterRows}
    columnCount={chapterColumns.length}
    estimateSize={68}
    tableClass="min-w-[680px]"
    itemLabel={$t("app.manga.chapters").toLocaleLowerCase()}
    emptyMessage={$t("app.manga.noChapters")}
    headerCellClass={(header) => chapterHeaderCellClass(header.column.id)}
    header={chapterHeader}
    row={chapterRow}
    mobileRow={chapterMobileRow}
  />
</SourceMangaChaptersCard>
