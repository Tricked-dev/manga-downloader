<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "../utils";

  let {
    id,
    label,
    description = "",
    class: className = "",
    labelClass = "",
    aside,
    children,
  }: {
    aside?: Snippet;
    children?: Snippet;
    class?: string;
    description?: string;
    id?: string;
    label: string;
    labelClass?: string;
  } = $props();
</script>

<div class={cn("space-y-1.5", className)}>
  <div class="flex items-center gap-2">
    <label for={id} class={cn("text-sm font-medium", labelClass)}>{label}</label>
    {#if aside}
      {@render aside()}
    {/if}
  </div>
  {#if description}
    <p class="text-xs text-muted-foreground">{description}</p>
  {/if}
  {#if children}
    {@render children()}
  {/if}
</div>
