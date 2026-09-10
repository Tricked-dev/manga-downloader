<script lang="ts">
  import {
    createFilteredRowModel,
    createSortedRowModel,
    createTable,
    filterFns,
    globalFilteringFeature,
    rowSelectionFeature,
    rowSortingFeature,
    sortFns,
    tableFeatures,
    type ColumnDef,
    type Header,
    type Row,
  } from "@tanstack/svelte-table";
  import { Check, Minus } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import DownloadMobileRow from "$components/features/downloads/DownloadMobileRow.svelte";
  import DownloadRowContent from "$components/features/downloads/DownloadRowContent.svelte";
  import DownloadsToolbarActions from "$components/features/downloads/DownloadsToolbarActions.svelte";
  import DownloadsToolbarFilters from "$components/features/downloads/DownloadsToolbarFilters.svelte";
  import TableSortHeader from "$components/common/TableSortHeader.svelte";
  import VirtualizedTanStackTable from "$components/common/VirtualizedTanStackTable.svelte";
  import {
    countCancelableDownloads,
    filterDownloadsByStatus,
    getBulkDownloadActionLabel,
    hasBusyDownload,
    type DownloadStatusFilter,
  } from "$lib/features/downloads/status";
  import type { DownloadItem } from "$lib/types";

  type SortOption = "queue" | "series-asc" | "series-desc";

  const {
    downloads,
    onRemove,
    onBulkRemove,
    removingIds,
  } = $props<{
    downloads: DownloadItem[];
    onRemove: (id: string) => void;
    onBulkRemove: (ids: string[]) => void | Promise<void>;
    removingIds: Set<string>;
  }>();

  let globalFilter = $state("");
  let statusFilter = $state<DownloadStatusFilter>("all");
  let sortOption = $state<SortOption>("queue");

  const _features = tableFeatures({
    globalFilteringFeature,
    rowSelectionFeature,
    rowSortingFeature,
  });
  type DownloadTableHeader = Header<typeof _features, DownloadItem>;
  type DownloadTableRow = Row<typeof _features, DownloadItem>;

  const columns: ColumnDef<typeof _features, DownloadItem>[] = [
    {
      id: "select",
      enableSorting: false,
    },
    {
      id: "series",
      accessorFn: (download: DownloadItem) => `${download.manga_title} ${download.manga_source}`,
      header: "app.downloads.series",
      sortFn: "alphanumeric",
    },
    {
      id: "chapter",
      accessorFn: (download: DownloadItem) => download.chapter_number,
      header: "app.downloads.chapter",
    },
    {
      accessorKey: "status",
      header: "app.downloads.status",
    },
    {
      accessorKey: "progress",
      header: "app.downloads.progress",
    },
    {
      id: "actions",
      enableSorting: false,
      header: "app.downloads.action",
    },
  ];

  const visibleDownloads = $derived(filterDownloadsByStatus(downloads, statusFilter));

  const table = createTable(
    {
      _features,
      _rowModels: {
        filteredRowModel: createFilteredRowModel(filterFns),
        sortedRowModel: createSortedRowModel(sortFns),
      },
      get data() {
        return visibleDownloads;
      },
      columns,
      globalFilterFn: "includesString",
      getRowId: (download: DownloadItem) => download.id,
    },
    (state) => state,
  );

  const filteredRows = $derived(table.getRowModel().rows);
  const selectedCount = $derived(table.getSelectedRowModel().rows.length);
  const selectedDownloads = $derived(table.getSelectedRowModel().rows.map((row) => row.original));
  const selectedCancelableCount = $derived(countCancelableDownloads(selectedDownloads));
  const bulkBusy = $derived(hasBusyDownload(selectedDownloads, removingIds));
  const bulkActionLabel = $derived(
    getBulkDownloadActionLabel(selectedCount, selectedCancelableCount, bulkBusy, $t),
  );

  function sortLabel(header: string | unknown): string {
    return typeof header === "string" ? $t(header) : "";
  }

  function selectionBoxClass(selected: boolean): string {
    const base = "flex size-4 items-center justify-center border transition-colors";
    if (selected) {
      return `${base} border-primary bg-primary text-primary-foreground`;
    }

    return `${base} border-muted-foreground/45 bg-card text-transparent group-hover:border-muted-foreground`;
  }

  function setStatusFilter(value: string) {
    statusFilter = value as DownloadStatusFilter;
    table.resetRowSelection();
  }

  function setSortOption(value: string) {
    sortOption = value as SortOption;

    if (sortOption === "queue") {
      table.resetSorting();
      return;
    }

    table.setSorting([{ id: "series", desc: sortOption === "series-desc" }]);
  }

  async function removeSelected() {
    if (selectedCount === 0) {
      return;
    }

    await onBulkRemove(table.getSelectedRowModel().rows.map((row) => row.original.id));
    table.resetRowSelection();
  }

  function updateGlobalFilter(value: string) {
    globalFilter = value;
    table.setGlobalFilter(value);
  }

  function headerCellClass(columnId: string): string {
    if (columnId === "actions") {
      return "w-[120px] pr-4 text-right";
    }

    if (columnId === "select") {
      return "w-12 pl-4";
    }

    return "";
  }

  function handleGlobalFilterInput(event: Event): void {
    updateGlobalFilter((event.currentTarget as HTMLInputElement).value);
  }

  function clearSelection(): void {
    table.resetRowSelection();
  }
</script>

{#snippet downloadToolbarFilters()}
  <DownloadsToolbarFilters
    {globalFilter}
    onGlobalFilterInput={handleGlobalFilterInput}
    {setSortOption}
    {setStatusFilter}
    {sortOption}
    {statusFilter}
  />
{/snippet}

{#snippet downloadToolbarActions()}
  <DownloadsToolbarActions
    {bulkActionLabel}
    {bulkBusy}
    clearSelectionLabel={$t("app.actions.clearSelection")}
    {clearSelection}
    {removeSelected}
    {selectedCancelableCount}
    {selectedCount}
  />
{/snippet}

{#snippet downloadHeader(header: DownloadTableHeader)}
  {#if header.column.id === "select"}
    {@const allRowsSelected = table.getIsAllRowsSelected()}
    {@const someRowsSelected = table.getIsSomeRowsSelected()}
    <label class="group inline-flex size-7 cursor-pointer items-center justify-center focus-within:ring-2 focus-within:ring-ring">
      <input
        type="checkbox"
        class="sr-only"
        checked={allRowsSelected}
        aria-checked={someRowsSelected ? "mixed" : allRowsSelected}
        aria-label={$t("app.downloads.selectAll")}
        onchange={table.getToggleAllRowsSelectedHandler()}
      />
      <span class={selectionBoxClass(allRowsSelected || someRowsSelected)}>
        {#if allRowsSelected}
          <Check class="size-3" />
        {:else if someRowsSelected}
          <Minus class="size-3" />
        {/if}
      </span>
    </label>
  {:else}
    <TableSortHeader
      label={sortLabel(header.column.columnDef.header)}
      canSort={header.column.getCanSort()}
      sorted={header.column.getIsSorted()}
      onclick={header.column.getToggleSortingHandler()}
      class={header.column.id === "actions" ? "ml-auto" : ""}
    />
  {/if}
{/snippet}

{#snippet downloadRow(row: DownloadTableRow, download: DownloadItem)}
  <DownloadRowContent {row} {download} {removingIds} {onRemove} {selectionBoxClass} />
{/snippet}

{#snippet downloadMobileRow(row: DownloadTableRow, download: DownloadItem)}
  <DownloadMobileRow {row} {download} {removingIds} {onRemove} {selectionBoxClass} />
{/snippet}

<VirtualizedTanStackTable
  {table}
  rows={filteredRows}
  columnCount={columns.length}
  tableClass="min-w-[760px]"
  itemLabel={$t("app.downloads.title").toLocaleLowerCase()}
  emptyMessage={$t("app.downloads.emptyFiltered")}
  toolbarFilters={downloadToolbarFilters}
  toolbarActions={downloadToolbarActions}
  toolbarFiltersClass="md:grid-cols-[minmax(16rem,24rem)_12rem_12rem]"
  headerCellClass={(header) => headerCellClass(header.column.id)}
  header={downloadHeader}
  row={downloadRow}
  mobileRow={downloadMobileRow}
/>
