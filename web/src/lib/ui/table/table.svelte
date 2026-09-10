<script lang="ts">
  import { cn } from "$lib/utils";

  const { class: className = "", children } = $props<{ class?: string; children?: () => unknown }>();
</script>

<div data-slot="table-container" class="relative w-full overflow-x-auto">
  <table data-slot="table" class={cn("w-full caption-bottom text-sm", className)}>
    {@render children?.()}
  </table>
</div>
