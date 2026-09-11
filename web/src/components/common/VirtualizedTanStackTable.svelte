<script lang="ts" generics="TFeatures extends TableFeatures, TData extends RowData">
  import { createVirtualizer, type VirtualItem } from "@tanstack/svelte-virtual";
  import type {
    Header,
    Row,
    RowData,
    SvelteTable,
    TableFeatures,
  } from "@tanstack/svelte-table";
  import { untrack, type Snippet } from "svelte";
  import {
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
  } from "$lib/ui/table";
  import TableToolbar from "$components/common/TableToolbar.svelte";
  import { t } from "$lib/i18n";
  import { MediaQuery } from "svelte/reactivity";

  type TableHeaderCell = Header<TFeatures, TData>;
  type TableRowItem = Row<TFeatures, TData>;

  const {
    table,
    rows,
    columnCount,
    estimateSize = 72,
    overscan = 8,
    containerClass = "max-h-[70vh] overflow-auto",
    tableClass = "",
    headerClass = "sticky top-0 z-10 bg-card",
    headerCellClass = () => "",
    itemLabel = "",
    emptyMessage = "",
    showFooter = true,
    toolbar,
    toolbarFilters,
    toolbarActions,
    toolbarClass = "",
    toolbarContentClass = "",
    toolbarFiltersClass = "",
    toolbarActionsClass = "",
    header,
    row,
    mobileRow,
    empty,
    footer,
  } = $props<{
    table: SvelteTable<TFeatures, TData, unknown>;
    rows: TableRowItem[];
    columnCount?: number;
    estimateSize?: number;
    overscan?: number;
    containerClass?: string;
    tableClass?: string;
    headerClass?: string;
    headerCellClass?: (header: TableHeaderCell) => string;
    itemLabel?: string;
    emptyMessage?: string;
    showFooter?: boolean;
    toolbar?: Snippet;
    toolbarFilters?: Snippet;
    toolbarActions?: Snippet;
    toolbarClass?: string;
    toolbarContentClass?: string;
    toolbarFiltersClass?: string;
    toolbarActionsClass?: string;
    header: Snippet<[TableHeaderCell]>;
    row: Snippet<[TableRowItem, TData, number, VirtualItem]>;
    mobileRow?: Snippet<[TableRowItem, TData, number]>;
    empty?: Snippet;
    footer?: Snippet<[number, number]>;
  }>();

  let tableScrollElement: HTMLDivElement | undefined;
  // Rows off screen are never measured, so they keep whatever estimate is in force. Feeding
  // the measured height back replaces the caller's guess for them too, which is what keeps
  // the scroll area from running past the last row before anyone has scrolled that far.
  let measuredRowSize = $state(0);
  const effectiveEstimateSize = $derived(measuredRowSize > 0 ? measuredRowSize : estimateSize);
  const mobileQuery = new MediaQuery("(max-width: 767px)");

  const rowVirtualizer = createVirtualizer<HTMLDivElement, HTMLTableRowElement>({
    count: 0,
    estimateSize: () => estimateSize,
    getScrollElement: () => tableScrollElement ?? null,
    overscan: 8,
  });
  const virtualRows = $derived($rowVirtualizer.getVirtualItems());
  const resolvedColumnCount = $derived(columnCount ?? table.getAllLeafColumns().length);
  const virtualPaddingTop = $derived(virtualRows[0]?.start ?? 0);
  const virtualPaddingBottom = $derived(
    Math.max(0, $rowVirtualizer.getTotalSize() - (virtualRows.at(-1)?.end ?? 0))
  );
  const footerSuffix = $derived(itemLabel ? ` ${itemLabel}` : "");
  const resolvedEmptyMessage = $derived(emptyMessage || $t("app.common.noRows"));
  const showMobileRows = $derived(Boolean(mobileRow && mobileQuery.current));

  $effect(() => {
    const count = rows.length;
    const scrollElement = tableScrollElement ?? null;
    const rowSize = effectiveEstimateSize;

    untrack(() => {
      $rowVirtualizer.setOptions({
        count,
        estimateSize: () => rowSize,
        getScrollElement: () => scrollElement,
        overscan,
      });
    });
  });

  // `estimateSize` is only a first guess, and a row that renders shorter than it leaves the
  // scroll area longer than the rows it holds: the scrollbar runs past the last chapter into
  // empty space, and the gap grows with every extra row. Measuring the rows that are on
  // screen replaces the guess with their real height.
  $effect(() => {
    const items = virtualRows;
    const scrollElement = tableScrollElement;
    if (!scrollElement) {
      return;
    }

    const rowElements = scrollElement.querySelectorAll<HTMLTableRowElement>(
      "tbody > tr:not([data-virtual-padding])",
    );
    let total = 0;
    let measured = 0;
    items.forEach((item, position) => {
      const element = rowElements[position];
      if (!element) {
        return;
      }
      element.dataset.index = String(item.index);
      $rowVirtualizer.measureElement(element);
      const height = element.getBoundingClientRect().height;
      if (height > 0) {
        total += height;
        measured += 1;
      }
    });

    if (measured === 0) {
      return;
    }
    const average = Math.round(total / measured);
    // Only a real difference is worth another pass; rounding noise would loop forever.
    if (Math.abs(average - effectiveEstimateSize) > 1) {
      measuredRowSize = average;
    }
  });

</script>

{#if toolbar}
  <TableToolbar class={toolbarClass} contentClass={toolbarContentClass}>
    {@render toolbar()}
  </TableToolbar>
{:else if toolbarFilters || toolbarActions}
  <TableToolbar
    class={toolbarClass}
    contentClass={toolbarContentClass}
    filtersClass={toolbarFiltersClass}
    actionsClass={toolbarActionsClass}
    filters={toolbarFilters}
    actions={toolbarActions}
  />
{/if}

<div bind:this={tableScrollElement} class={containerClass}>
  {#if showMobileRows && mobileRow}
    <div class="divide-y divide-border md:hidden">
      {#if rows.length === 0}
        <div class="px-4 py-10 text-center text-sm text-muted-foreground">
          {#if empty}
            {@render empty()}
          {:else}
            {resolvedEmptyMessage}
          {/if}
        </div>
      {:else}
        {#each rows as tableRow, index (tableRow.id)}
          {@render mobileRow(tableRow, tableRow.original, index)}
        {/each}
      {/if}
    </div>
  {/if}

  <table
    data-slot="table"
    class={`${mobileRow ? "hidden md:table " : ""}w-full caption-bottom text-sm ${tableClass}`}
  >
    <TableHeader class={headerClass}>
      {#each table.getHeaderGroups() as headerGroup (headerGroup.id)}
        <TableRow class="hover:bg-transparent">
          {#each headerGroup.headers as tableHeader (tableHeader.id)}
            <TableHead class={headerCellClass(tableHeader)}>
              {@render header(tableHeader)}
            </TableHead>
          {/each}
        </TableRow>
      {/each}
    </TableHeader>
    <TableBody>
      {#if virtualPaddingTop > 0}
        <TableRow data-virtual-padding="top" class="hover:bg-transparent">
          <td colspan={resolvedColumnCount} class="h-[var(--virtual-padding)] p-0" style:--virtual-padding={`${virtualPaddingTop}px`}></td>
        </TableRow>
      {/if}
      {#each virtualRows as virtualRow (virtualRow.key)}
        {@const tableRow = rows[virtualRow.index]}
        {#if tableRow}
          {@render row(tableRow, tableRow.original, virtualRow.index, virtualRow)}
        {/if}
      {:else}
        {#if rows.length === 0}
          <TableRow>
            <TableCell colspan={resolvedColumnCount} class="py-10 text-center text-sm text-muted-foreground">
              {#if empty}
                {@render empty()}
              {:else}
                {resolvedEmptyMessage}
              {/if}
            </TableCell>
          </TableRow>
        {/if}
      {/each}
      {#if virtualPaddingBottom > 0}
        <TableRow data-virtual-padding="bottom" class="hover:bg-transparent">
          <td colspan={resolvedColumnCount} class="h-[var(--virtual-padding)] p-0" style:--virtual-padding={`${virtualPaddingBottom}px`}></td>
        </TableRow>
      {/if}
    </TableBody>
  </table>
</div>

{#if showFooter && rows.length > 0}
  <div class="border-t border-border/70 px-4 py-3 text-xs text-muted-foreground">
    {#if footer}
      {@render footer(virtualRows.length, rows.length)}
    {:else}
      {$t("app.table.showing", { visible: virtualRows.length, total: rows.length, suffix: footerSuffix })}
    {/if}
  </div>
{/if}
