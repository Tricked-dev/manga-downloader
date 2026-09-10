<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { ArrowDown, ArrowUp, ChevronsUpDown } from "@lucide/svelte";

  const {
    label,
    canSort = false,
    sorted = false,
    onclick,
    class: className = "",
  } = $props<{
    label: string;
    canSort?: boolean;
    sorted?: false | "asc" | "desc";
    onclick?: (event: MouseEvent) => void;
    class?: string;
  }>();
</script>

{#if canSort}
  <Button
    variant="ghost"
    size="xs"
    class={`-ml-2 h-7 px-2 text-xs font-semibold uppercase tracking-[0.16em] text-muted-foreground hover:text-foreground ${className}`}
    {onclick}
  >
    {label}
    {#if sorted === "asc"}
      <ArrowUp class="h-3.5 w-3.5" />
    {:else if sorted === "desc"}
      <ArrowDown class="h-3.5 w-3.5" />
    {:else}
      <ChevronsUpDown class="h-3.5 w-3.5 opacity-60" />
    {/if}
  </Button>
{:else}
  <span class={className}>{label}</span>
{/if}
