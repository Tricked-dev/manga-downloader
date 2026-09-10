<script lang="ts">
  import { Trash2, X } from "@lucide/svelte";
  import { Button } from "$lib/ui/button";

  const {
    bulkActionLabel,
    bulkBusy,
    clearSelectionLabel,
    clearSelection,
    removeSelected,
    selectedCancelableCount,
    selectedCount,
  }: {
    bulkActionLabel: string;
    bulkBusy: boolean;
    clearSelectionLabel: string;
    clearSelection: () => void;
    removeSelected: () => void | Promise<void>;
    selectedCancelableCount: number;
    selectedCount: number;
  } = $props();
</script>

<Button
  variant="destructive"
  size="icon-lg"
  aria-label={bulkActionLabel}
  title={bulkActionLabel}
  disabled={selectedCount === 0 || bulkBusy}
  onclick={removeSelected}
>
  {#if selectedCancelableCount > 0}
    <X class="h-3.5 w-3.5" />
  {:else}
    <Trash2 class="h-3.5 w-3.5" />
  {/if}
</Button>
<Button
  variant="outline"
  size="icon-lg"
  aria-label={clearSelectionLabel}
  title={clearSelectionLabel}
  disabled={selectedCount === 0}
  onclick={clearSelection}
>
  <X class="h-3.5 w-3.5" />
</Button>
