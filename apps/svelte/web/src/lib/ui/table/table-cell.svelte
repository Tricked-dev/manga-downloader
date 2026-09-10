<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLTdAttributes } from "svelte/elements";

  const {
    class: className = "",
    children,
    ...restProps
  } = $props<{ class?: string; children?: () => unknown } & HTMLTdAttributes>();
</script>

<td data-slot="table-cell" class={cn("p-3 align-middle", className)} {...restProps}>
  {@render children?.()}
</td>
