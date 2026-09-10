<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";

  const {
    class: className = "",
    contentClass = "",
    filtersClass = "",
    actionsClass = "",
    children,
    filters,
    actions,
  } = $props<{
    class?: string;
    contentClass?: string;
    filtersClass?: string;
    actionsClass?: string;
    children?: Snippet;
    filters?: Snippet;
    actions?: Snippet;
  }>();
</script>

<div class={cn("border-b border-border/70 bg-card px-4 py-3", className)}>
  {#if children}
    {@render children()}
  {:else}
    <div class={cn("flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between", contentClass)}>
      {#if filters}
        <div class={cn("grid min-w-0 flex-1 gap-2", filtersClass)}>
          {@render filters()}
        </div>
      {/if}
      {#if actions}
        <div class={cn("flex shrink-0 flex-wrap items-center justify-end gap-2", actionsClass)}>
          {@render actions()}
        </div>
      {/if}
    </div>
  {/if}
</div>
