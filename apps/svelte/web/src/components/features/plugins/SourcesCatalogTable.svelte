<script lang="ts">
  import { base } from "$app/paths";
  import {
    createSortedRowModel,
    createTable,
    rowSortingFeature,
    sortFns,
    tableFeatures,
    type ColumnDef,
    type Header,
    type Row,
  } from "@tanstack/svelte-table";
  import { TableCell, TableRow } from "$lib/ui/table";
  import TableSortHeader from "$components/common/TableSortHeader.svelte";
  import VirtualizedTanStackTable from "$components/common/VirtualizedTanStackTable.svelte";
  import SourceCapabilitiesBadges from "$components/features/plugins/SourceCapabilitiesBadges.svelte";
  import SourceSummaryCell from "$components/features/plugins/SourceSummaryCell.svelte";
  import CatalogSourceActions from "$components/features/plugins/CatalogSourceActions.svelte";
  import { createSourceInfoOpenState } from "$components/features/plugins/source-info-open-state.svelte";
  import { t } from "$lib/i18n";
  import type { Source, SourceSettings } from "$lib/types";

  const {
    sources,
    sourceSettings,
    loadingSettings,
    savingSettings,
    onOpenInfo,
    onHideNsfwChange,
  } = $props<{
    sources: Source[];
    sourceSettings: Record<string, SourceSettings>;
    loadingSettings: Record<string, boolean>;
    savingSettings: Record<string, boolean>;
    onOpenInfo: (source: Source) => void | Promise<void>;
    onHideNsfwChange: (source: Source, checked: boolean) => void;
  }>();

  const sourceInfoOpen = createSourceInfoOpenState((source) => onOpenInfo(source));
  const _features = tableFeatures({
    rowSortingFeature,
  });
  type SourcesTableHeader = Header<typeof _features, Source>;
  type SourcesTableRow = Row<typeof _features, Source>;
  const sourceIconUrls: Record<string, string> = {
    comix: `${base}/source-icons/comix.avif`,
  };

  const columns: ColumnDef<typeof _features, Source>[] = [
    {
      id: "source",
      accessorFn: (source: Source) => `${source.display_name} ${source.base_url}`,
      header: "app.sources.title",
    },
    {
      id: "capabilities",
      accessorFn: (source: Source) => source.capabilities.join(" "),
      header: "app.sources.capabilities",
    },
    {
      id: "browse",
      enableSorting: false,
      header: "app.sources.browse",
    },
  ];

  const table = createTable(
    {
      _features,
      _rowModels: {
        sortedRowModel: createSortedRowModel(sortFns),
      },
      get data() {
        return sources;
      },
      columns,
      getRowId: (source: Source) => source.name,
    },
    (state) => state,
  );
  const rows = $derived(table.getRowModel().rows);

  function sortLabel(header: string | unknown): string {
    return typeof header === "string" ? $t(header) : "";
  }

  function sourceIconUrl(source: Source): string | null {
    return sourceIconUrls[source.name] ?? null;
  }

</script>

{#snippet sourceHeader(header: SourcesTableHeader)}
  <TableSortHeader
    label={sortLabel(header.column.columnDef.header)}
    canSort={header.column.getCanSort()}
    sorted={header.column.getIsSorted()}
    onclick={header.column.getToggleSortingHandler()}
    class={header.column.id === "browse" ? "ml-auto" : ""}
  />
{/snippet}

{#snippet sourceRow(row: SourcesTableRow, source: Source)}
  <TableRow>
        {#each row.getAllCells() as cell (cell.id)}
          {#if cell.column.id === "source"}
            <TableCell>
              <SourceSummaryCell source={source} iconUrl={sourceIconUrl(source)} truncateBaseUrl />
            </TableCell>
          {:else if cell.column.id === "capabilities"}
            <TableCell>
              <SourceCapabilitiesBadges capabilities={source.capabilities} />
            </TableCell>
          {:else if cell.column.id === "browse"}
            <TableCell class="text-right">
              <div class="flex items-center justify-end gap-2">
                <CatalogSourceActions
                  {source}
                  {sourceSettings}
                  {loadingSettings}
                  {savingSettings}
                  {sourceInfoOpen}
                  {onHideNsfwChange}
                />
              </div>
            </TableCell>
          {/if}
        {/each}
  </TableRow>
{/snippet}

{#snippet sourceMobileRow(_row: SourcesTableRow, source: Source)}
  <article class="space-y-4 bg-card p-4">
    <SourceSummaryCell source={source} iconUrl={sourceIconUrl(source)} />

    <SourceCapabilitiesBadges capabilities={source.capabilities} />

    <div class="flex items-center justify-end gap-2">
      <CatalogSourceActions
        {source}
        {sourceSettings}
        {loadingSettings}
        {savingSettings}
        {sourceInfoOpen}
        {onHideNsfwChange}
      />
    </div>
  </article>
{/snippet}

<VirtualizedTanStackTable
  {table}
  {rows}
  columnCount={columns.length}
  itemLabel={$t("app.sources.title").toLocaleLowerCase()}
  headerCellClass={(header) => header.column.id === "browse" ? "w-[120px] text-right" : ""}
  header={sourceHeader}
  row={sourceRow}
  mobileRow={sourceMobileRow}
/>
