<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    description,
    action,
  }: {
    action?: Snippet;
    description?: string;
    title: string;
  } = $props();
</script>

<div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
  <div>
    <h2 class="text-base font-semibold">{title}</h2>
    {#if description}
      <p class="text-sm text-muted-foreground">{description}</p>
    {/if}
  </div>
  {#if action}
    <div class="flex shrink-0 flex-wrap items-center gap-2">
      {@render action()}
    </div>
  {/if}
</div>
