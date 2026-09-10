<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    checked = $bindable(false),
    children,
  }: {
    checked: boolean;
    children: Snippet;
  } = $props();
</script>

<label class="flex items-center gap-3 border border-border bg-background p-3 text-sm">
  <input class="size-4 accent-primary" type="checkbox" bind:checked />
  <span>{@render children()}</span>
</label>
