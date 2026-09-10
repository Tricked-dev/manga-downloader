<script lang="ts">
  import TableSortHeader from "$components/common/TableSortHeader.svelte";
  import { t } from "$lib/i18n";
  import { Check, Minus } from "@lucide/svelte";

  let {
    chapterTable,
    header,
    publicView = false,
    selectionBoxClass,
    sortLabel,
  } = $props<{
    chapterTable: any;
    header: any;
    publicView?: boolean;
    selectionBoxClass: (selected: boolean) => string;
    sortLabel: (header: string | unknown) => string;
  }>();
</script>

{#if header.column.id === "select" && !publicView}
  {@const allRowsSelected = chapterTable.getIsAllRowsSelected()}
  {@const someRowsSelected = chapterTable.getIsSomeRowsSelected()}
  <label class="group inline-flex size-7 cursor-pointer items-center justify-center focus-within:ring-2 focus-within:ring-ring">
    <input
      type="checkbox"
      class="sr-only"
      checked={allRowsSelected}
      aria-checked={someRowsSelected ? "mixed" : allRowsSelected}
      aria-label={$t("app.libraryDetail.selectAll")}
      onchange={chapterTable.getToggleAllRowsSelectedHandler()}
    />
    <span class={selectionBoxClass(allRowsSelected || someRowsSelected)}>
      {#if allRowsSelected}
        <Check class="size-3" />
      {:else if someRowsSelected}
        <Minus class="size-3" />
      {/if}
    </span>
  </label>
{:else if header.column.id !== "select"}
  <TableSortHeader
    label={sortLabel(header.column.columnDef.header)}
    canSort={header.column.getCanSort()}
    sorted={header.column.getIsSorted()}
    onclick={header.column.getToggleSortingHandler()}
    class={header.column.id === "actions" ? "ml-auto" : ""}
  />
{/if}
