<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLThAttributes } from "svelte/elements";

  const {
    class: className = "",
    children,
    ...restProps
  } = $props<{ class?: string; children?: () => unknown } & HTMLThAttributes>();
</script>

<th
  data-slot="table-head"
  class={cn("h-10 px-3 text-left align-middle text-xs font-semibold uppercase tracking-[0.16em] text-muted-foreground", className)}
  {...restProps}
>
  {@render children?.()}
</th>
