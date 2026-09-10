<script lang="ts">
  import { AlertTriangle, CheckCircle2 } from "@lucide/svelte";

  let {
    configured,
    label,
    missing,
  }: {
    configured: boolean;
    label: string;
    missing: string;
  } = $props();
</script>

<div class={configured ? "panel flex items-start gap-3 p-3" : "panel flex items-start gap-3 bg-destructive/10 p-3 text-destructive ring-destructive/30"}>
  {#if configured}
    <CheckCircle2 class="mt-0.5 size-4 text-primary" />
  {:else}
    <AlertTriangle class="mt-0.5 size-4" />
  {/if}
  <div class="min-w-0 text-sm">
    <p class="font-medium">{label}</p>
    {#if !configured}
      <p class="mt-1 text-xs opacity-85">{missing}</p>
    {/if}
  </div>
</div>
