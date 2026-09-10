<script lang="ts">
  import { cn } from "$lib/utils";
  const { class: className = "", children } = $props<{ class?: string; children?: () => unknown }>();
</script>

<tbody data-slot="table-body" class={cn("[&_tr:last-child]:border-0", className)}>
  {@render children?.()}
</tbody>
