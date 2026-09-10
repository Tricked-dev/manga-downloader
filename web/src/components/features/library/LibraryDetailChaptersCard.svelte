<script lang="ts">
  import { Card, CardAction, CardContent, CardHeader, CardTitle } from "$lib/ui/card";
  import { Button } from "$lib/ui/button";
  import LoadingState from "$components/common/LoadingState.svelte";
  import VirtualizedTanStackTable from "$components/common/VirtualizedTanStackTable.svelte";
  import LibraryChapterBulkActions from "$components/features/library/LibraryChapterBulkActions.svelte";
  import LibraryChapterDesktopRow from "$components/features/library/LibraryChapterDesktopRow.svelte";
  import LibraryChapterMobileRow from "$components/features/library/LibraryChapterMobileRow.svelte";
  import LibraryChapterTableHeader from "$components/features/library/LibraryChapterTableHeader.svelte";
  import { t } from "$lib/i18n";
  import { RefreshCw } from "@lucide/svelte";
  import type { ChapterTableRow } from "$lib/features/library/library-chapter-actions";
  import type {
    ColumnDef,
    Header,
    Row,
    SvelteTable,
    TableFeatures,
  } from "@tanstack/svelte-table";

  type ChapterTable = SvelteTable<TableFeatures, ChapterTableRow, unknown>;
  type ChapterTableHeader = Header<TableFeatures, ChapterTableRow>;
  type ChapterTableRowItem = Row<TableFeatures, ChapterTableRow>;

  let {
    chapterActionMessage,
    chapterActionMessageIsError,
    chapterColumns,
    chapterHeaderCellClass,
    chapterReadLabel,
    chapterTable,
    chaptersError,
    chaptersLoading,
    clearActionDisabled,
    comicInfoRefreshMessage,
    comicInfoRefreshMessageIsError,
    deleteActionDisabled,
    disabledActionClass,
    downloadActionDisabled,
    mangaSource,
    markReadActionDisabled,
    markUnreadActionDisabled,
    upscaleActionDisabled,
    refreshChaptersPending,
    refreshComicInfoPending,
    refreshMessage,
    refreshMessageIsError,
    publicView = false,
    selectedCount,
    selectedDeleteLoading,
    selectedReadLoading,
    selectedUpscaleLoading,
    selectedUnreadLoading,
    bulkDownloadLoading,
    selectionBoxClass,
    sortedChapterRows,
    sortLabel,
    onClearSelection,
    onDeleteSelected,
    onDownloadSelected,
    onMarkSelectedRead,
    onMarkSelectedUnread,
    onUpscaleSelected,
    onRefreshChapters,
    onRefreshComicInfo,
  } = $props<{
    bulkDownloadLoading: boolean;
    chapterActionMessage: string;
    chapterActionMessageIsError: boolean;
    chapterColumns: ColumnDef<TableFeatures, ChapterTableRow>[];
    chapterHeaderCellClass: (columnId: string) => string;
    chapterReadLabel: (chapter: ChapterTableRow["chapter"]) => string;
    chapterTable: ChapterTable;
    chaptersError: string;
    chaptersLoading: boolean;
    clearActionDisabled: boolean;
    comicInfoRefreshMessage: string;
    comicInfoRefreshMessageIsError: boolean;
    deleteActionDisabled: boolean;
    disabledActionClass: (disabled: boolean) => string;
    downloadActionDisabled: boolean;
    mangaSource: string;
    markReadActionDisabled: boolean;
    markUnreadActionDisabled: boolean;
    upscaleActionDisabled: boolean;
    refreshChaptersPending: boolean;
    refreshComicInfoPending: boolean;
    refreshMessage: string;
    refreshMessageIsError: boolean;
    publicView?: boolean;
    selectedCount: number;
    selectedDeleteLoading: boolean;
    selectedReadLoading: boolean;
    selectedUpscaleLoading: boolean;
    selectedUnreadLoading: boolean;
    selectionBoxClass: (selected: boolean) => string;
    sortedChapterRows: ChapterTableRowItem[];
    sortLabel: (header: string | unknown) => string;
    onClearSelection: (event: MouseEvent) => void;
    onDeleteSelected: (event: MouseEvent) => void;
    onDownloadSelected: (event: MouseEvent) => void;
    onMarkSelectedRead: (event: MouseEvent) => void;
    onMarkSelectedUnread: (event: MouseEvent) => void;
    onUpscaleSelected: (event: MouseEvent) => void;
    onRefreshChapters: () => void | Promise<void>;
    onRefreshComicInfo: () => void | Promise<void>;
  }>();
</script>

{#snippet chapterHeader(header: ChapterTableHeader)}
  <LibraryChapterTableHeader
    {chapterTable}
    {header}
    {publicView}
    {selectionBoxClass}
    {sortLabel}
  />
{/snippet}

{#snippet chapterRow(row: ChapterTableRowItem, chapterTableRow: ChapterTableRow)}
  <LibraryChapterDesktopRow
    {row}
    {chapterTableRow}
    {mangaSource}
    {chapterReadLabel}
    {publicView}
    {selectionBoxClass}
  />
{/snippet}

{#snippet chapterMobileRow(row: ChapterTableRowItem, chapterTableRow: ChapterTableRow)}
  <LibraryChapterMobileRow
    {row}
    {chapterTableRow}
    {mangaSource}
    {chapterReadLabel}
    {publicView}
    {selectionBoxClass}
  />
{/snippet}

<Card class="gap-0">
  <CardHeader class="border-b border-border/70 gap-4 sm:flex-row sm:items-start sm:justify-between">
    <div class="space-y-1">
      <CardTitle>{$t("app.manga.chapters")}</CardTitle>
    </div>
    {#if !publicView}
      <CardAction class="flex flex-wrap justify-end gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={refreshChaptersPending}
          onclick={onRefreshChapters}
        >
          <RefreshCw class={`h-4 w-4 ${refreshChaptersPending ? "animate-spin" : ""}`} />
          {refreshChaptersPending ? $t("app.libraryDetail.refreshing") : $t("app.libraryDetail.refreshChapters")}
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={refreshComicInfoPending}
          onclick={onRefreshComicInfo}
        >
          <RefreshCw class={`h-4 w-4 ${refreshComicInfoPending ? "animate-spin" : ""}`} />
          {refreshComicInfoPending ? $t("app.libraryDetail.refreshing") : $t("app.libraryDetail.refreshComicInfo")}
        </Button>
      </CardAction>
    {/if}
  </CardHeader>
  <CardContent class="p-0">
    {#if !publicView}
      <LibraryChapterBulkActions
        {clearActionDisabled}
        {deleteActionDisabled}
        {downloadActionDisabled}
        {upscaleActionDisabled}
        {markReadActionDisabled}
        {markUnreadActionDisabled}
        {selectedCount}
        {selectedDeleteLoading}
        {selectedReadLoading}
        {selectedUpscaleLoading}
        {selectedUnreadLoading}
        {bulkDownloadLoading}
        {disabledActionClass}
        onClear={onClearSelection}
        onDelete={onDeleteSelected}
        onDownload={onDownloadSelected}
        onMarkRead={onMarkSelectedRead}
        onMarkUnread={onMarkSelectedUnread}
        onUpscale={onUpscaleSelected}
      />
    {/if}
    {#if chapterActionMessage}
      <p class={`border-b border-border px-5 py-3 text-sm ${chapterActionMessageIsError ? "text-destructive" : "text-muted-foreground"}`}>{chapterActionMessage}</p>
    {/if}
    {#if refreshMessage}
      <p class={`border-b border-border px-5 py-3 text-sm ${refreshMessageIsError ? "text-destructive" : "text-muted-foreground"}`}>{refreshMessage}</p>
    {/if}
    {#if comicInfoRefreshMessage}
      <p class={`border-b border-border px-5 py-3 text-sm ${comicInfoRefreshMessageIsError ? "text-destructive" : "text-muted-foreground"}`}>{comicInfoRefreshMessage}</p>
    {/if}
    {#if chaptersLoading}
      <div class="px-5 py-6">
        <LoadingState label={$t("app.libraryDetail.chapterDataLoading")} compact />
      </div>
    {:else if chaptersError}
      <p class="px-5 py-4 text-sm text-destructive">{chaptersError}</p>
    {:else}
      <VirtualizedTanStackTable
        table={chapterTable}
        rows={sortedChapterRows}
        columnCount={chapterColumns.length}
        estimateSize={74}
        tableClass="min-w-[860px]"
        itemLabel={$t("app.manga.chapters").toLocaleLowerCase()}
        emptyMessage={$t("app.manga.noChapters")}
        headerCellClass={(header) => chapterHeaderCellClass(header.column.id)}
        header={chapterHeader}
        row={chapterRow}
        mobileRow={chapterMobileRow}
      />
    {/if}
  </CardContent>
</Card>
