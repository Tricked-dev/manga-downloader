<script lang="ts">
  import type { Snippet } from "svelte";

  import { cn } from "$lib/utils";

  const {
    children,
    class: className = "",
    variant = "warning",
  } = $props<{
    children?: Snippet;
    class?: string;
    variant?: "warning";
  }>();

  const variantClass = $derived(
    variant === "warning"
      ? "border-amber-500/30 bg-amber-500/10 text-amber-800 dark:text-amber-200"
      : "",
  );
</script>

<div class={cn("rounded-lg border px-3 py-2 text-xs", variantClass, className)}>
  {#if children}
    {@render children()}
  {/if}
</div>
