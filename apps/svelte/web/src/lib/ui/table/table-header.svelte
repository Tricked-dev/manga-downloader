<script lang="ts">
  import { cn } from "$lib/utils";
  const { class: className = "", children } = $props<{ class?: string; children?: () => unknown }>();
</script>

<thead data-slot="table-header" class={cn("[&_tr]:border-b", className)}>
  {@render children?.()}
</thead>
