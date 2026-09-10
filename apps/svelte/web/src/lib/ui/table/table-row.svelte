<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  const {
    class: className = "",
    children,
    ...restProps
  } = $props<{
    class?: string;
    children?: () => unknown;
  } & HTMLAttributes<HTMLTableRowElement>>();
</script>

<tr
  data-slot="table-row"
  class={cn("border-b border-border/70 transition-colors hover:bg-muted/40 data-[state=selected]:bg-muted", className)}
  {...restProps}
>
  {@render children?.()}
</tr>
