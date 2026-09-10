<script lang="ts">
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
  import { Badge } from "$lib/ui/badge";
  import { TableCell, TableRow } from "$lib/ui/table";
  import TableSortHeader from "$components/common/TableSortHeader.svelte";
  import VirtualizedTanStackTable from "$components/common/VirtualizedTanStackTable.svelte";
  import InstalledSourceActions from "$components/features/plugins/InstalledSourceActions.svelte";
  import SourceSummaryCell from "$components/features/plugins/SourceSummaryCell.svelte";
  import { createSourceInfoOpenState } from "$components/features/plugins/source-info-open-state.svelte";
  import { t } from "$lib/i18n";
  import type { PluginArtifact, Source, SourceSettings } from "$lib/types";

  const {
    sources,
    toggling,
    sourceSettings,
    sourceArtifacts,
    loadingSettings,
    loadingArtifacts,
    savingSettings,
    deletingSources,
    onOpenInfo,
    onHideNsfwChange,
    onDelete,
    onToggle,
  } = $props<{
    sources: Source[];
    toggling: Record<string, boolean>;
    sourceSettings: Record<string, SourceSettings>;
    sourceArtifacts: Record<string, PluginArtifact[]>;
    loadingSettings: Record<string, boolean>;
    loadingArtifacts: Record<string, boolean>;
    savingSettings: Record<string, boolean>;
    deletingSources: Record<string, boolean>;
    onOpenInfo: (source: Source) => void | Promise<void>;
    onHideNsfwChange: (source: Source, checked: boolean) => void;
    onDelete: (source: Source) => void;
    onToggle: (source: Source, enabled: boolean) => void;
  }>();

  const sourceInfoOpen = createSourceInfoOpenState((source) => onOpenInfo(source));
  const _features = tableFeatures({
    rowSortingFeature,
  });
  type InstalledSourcesTableHeader = Header<typeof _features, Source>;
  type InstalledSourcesTableRow = Row<typeof _features, Source>;

  const columns: ColumnDef<typeof _features, Source>[] = [
    {
      id: "plugin",
      accessorFn: (source: Source) => `${source.display_name} ${source.name} ${source.base_url}`,
      header: "app.sources.plugin",
    },
    {
      accessorKey: "plugin_version",
      header: "app.about.version",
    },
    {
      accessorKey: "plugin_api_version",
      header: "app.sources.pluginApi",
    },
    {
      accessorKey: "enabled",
      header: "app.sources.status",
    },
    {
      id: "enabledAction",
      enableSorting: false,
      header: "app.sources.enabled",
    },
    {
      id: "info",
      enableSorting: false,
      header: "app.sources.info",
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

  function statusLabel(source: Source) {
    return source.enabled ? "active" : "disabled";
  }

  function sortLabel(header: string | unknown): string {
    return typeof header === "string" ? $t(header) : "";
  }

</script>

{#snippet pluginHeader(header: InstalledSourcesTableHeader)}
  <TableSortHeader
    label={sortLabel(header.column.columnDef.header)}
    canSort={header.column.getCanSort()}
    sorted={header.column.getIsSorted()}
    onclick={header.column.getToggleSortingHandler()}
    class={header.column.id === "enabledAction" || header.column.id === "info" ? "ml-auto" : ""}
  />
{/snippet}

{#snippet pluginRow(row: InstalledSourcesTableRow, source: Source)}
  <TableRow>
        {#each row.getAllCells() as cell (cell.id)}
          {#if cell.column.id === "plugin"}
            <TableCell>
              <SourceSummaryCell source={source} showKey />
            </TableCell>
          {:else if cell.column.id === "plugin_version"}
            <TableCell>
              <p class="text-sm">{source.plugin_version}</p>
            </TableCell>
          {:else if cell.column.id === "plugin_api_version"}
            <TableCell>
              <p class="text-xs text-muted-foreground">{source.plugin_api_version}</p>
            </TableCell>
          {:else if cell.column.id === "enabled"}
            <TableCell>
              <Badge variant="outline">{statusLabel(source)}</Badge>
            </TableCell>
          {:else if cell.column.id === "enabledAction"}
            <TableCell class="text-right">
              <div class="flex justify-end">
                <InstalledSourceActions
                  {source}
                  {toggling}
                  {sourceSettings}
                  {sourceArtifacts}
                  {loadingSettings}
                  {loadingArtifacts}
                  {savingSettings}
                  {deletingSources}
                  {sourceInfoOpen}
                  {onHideNsfwChange}
                  {onDelete}
                  {onToggle}
                  showInfo={false}
                />
              </div>
            </TableCell>
          {:else if cell.column.id === "info"}
            <TableCell class="text-right">
              <InstalledSourceActions
                {source}
                {toggling}
                {sourceSettings}
                {sourceArtifacts}
                {loadingSettings}
                {loadingArtifacts}
                {savingSettings}
                {deletingSources}
                {sourceInfoOpen}
                {onHideNsfwChange}
                {onDelete}
                {onToggle}
                showToggle={false}
              />
            </TableCell>
          {/if}
        {/each}
  </TableRow>
{/snippet}

{#snippet pluginMobileRow(_row: InstalledSourcesTableRow, source: Source)}
  <article class="space-y-4 bg-card p-4">
    <SourceSummaryCell source={source} showKey />

    <div class="grid grid-cols-2 gap-3 text-sm">
      <div class="min-w-0">
        <p class="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">{$t("app.about.version")}</p>
        <p class="truncate">{source.plugin_version}</p>
      </div>
      <div class="min-w-0">
        <p class="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">{$t("app.sources.pluginApi")}</p>
        <p class="truncate text-xs text-muted-foreground">{source.plugin_api_version}</p>
      </div>
    </div>

    <div class="flex items-center justify-between gap-3">
      <Badge variant="outline">{statusLabel(source)}</Badge>
      <div class="flex items-center gap-3">
        <InstalledSourceActions
          {source}
          {toggling}
          {sourceSettings}
          {sourceArtifacts}
          {loadingSettings}
          {loadingArtifacts}
          {savingSettings}
          {deletingSources}
          {sourceInfoOpen}
          {onHideNsfwChange}
          {onDelete}
          {onToggle}
        />
      </div>
    </div>
  </article>
{/snippet}

<VirtualizedTanStackTable
  {table}
  {rows}
  columnCount={columns.length}
  estimateSize={76}
  itemLabel={$t("app.sources.plugins").toLocaleLowerCase()}
  headerCellClass={(header) => header.column.id === "enabledAction" || header.column.id === "info" ? "text-right" : ""}
  header={pluginHeader}
  row={pluginRow}
  mobileRow={pluginMobileRow}
/>
