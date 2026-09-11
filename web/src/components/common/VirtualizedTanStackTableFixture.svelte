<script lang="ts">
  import { createTable, tableFeatures, type ColumnDef, type Header, type Row } from "@tanstack/svelte-table";
  import { TableCell, TableRow } from "$lib/ui/table";
  import VirtualizedTanStackTable from "./VirtualizedTanStackTable.svelte";

  type FixtureRow = { id: string; label: string };

  const { rowCount = 200, estimateSize = 74, rowHeight = 40 } = $props<{
    rowCount?: number;
    estimateSize?: number;
    rowHeight?: number;
  }>();

  const _features = tableFeatures({});
  type FixtureHeader = Header<typeof _features, FixtureRow>;
  type FixtureTableRow = Row<typeof _features, FixtureRow>;

  const columns: ColumnDef<typeof _features, FixtureRow>[] = [
    { accessorKey: "label", header: "Label", id: "label" },
  ];

  const data = $derived(
    Array.from({ length: rowCount }, (_, index) => ({
      id: `row-${index}`,
      label: `Row ${index}`,
    })),
  );

  const table = createTable({
    _features,
    get columns() {
      return columns;
    },
    get data() {
      return data;
    },
    getRowId: (row: FixtureRow) => row.id,
    renderFallbackValue: "",
  });

  const rows = $derived(table.getRowModel().rows);
</script>

{#snippet fixtureHeader(header: FixtureHeader)}
  {header.column.id}
{/snippet}

{#snippet fixtureRow(_row: FixtureTableRow, original: FixtureRow)}
  <TableRow>
    <TableCell class="p-0">
      <div style:height={`${rowHeight}px`}>{original.label}</div>
    </TableCell>
  </TableRow>
{/snippet}

<!-- Tailwind classes are not compiled into this harness, so the scroll box and the table
     reset the component relies on are spelled out here. Without them nothing virtualizes. -->
<div data-testid="fixture">
  <VirtualizedTanStackTable
    {table}
    {rows}
    {estimateSize}
    columnCount={1}
    containerClass="fixture-scroll"
    showFooter={false}
    header={fixtureHeader}
    row={fixtureRow}
  />
</div>

<style>
  :global(.fixture-scroll) {
    height: 400px;
    overflow: auto;
  }

  :global(.fixture-scroll table) {
    border-collapse: collapse;
    width: 100%;
  }
</style>
