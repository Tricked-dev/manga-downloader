<script lang="ts">
  import HealthChecksPopover from "$components/features/about/HealthChecksPopover.svelte";
  import { cn } from "$lib/utils";
  import type { StatusItem, StatusTone } from "$lib/features/about/types";
  import { Activity, GitBranch, GitCommit } from "@lucide/svelte";

  let { item } = $props<{ item: StatusItem }>();

  function statusPanelClass(tone: StatusTone): string {
    if (tone === "ok") {
      return "border-l-2 border-l-emerald-500/80";
    }
    if (tone === "warn") {
      return "border-l-2 border-l-destructive/80";
    }

    return "border-l-2 border-l-muted-foreground/50";
  }

  function statusIconClass(tone: StatusTone): string {
    if (tone === "ok") {
      return "text-emerald-400";
    }
    if (tone === "warn") {
      return "text-destructive";
    }

    return "text-muted-foreground";
  }

  function statusDotClass(tone: StatusTone): string {
    if (tone === "ok") {
      return "bg-emerald-400";
    }
    if (tone === "warn") {
      return "bg-destructive";
    }

    return "bg-muted-foreground";
  }
</script>

<div
  class={cn(
    "group relative border border-border/70 bg-muted/20 p-3.5 transition-colors hover:bg-muted/30",
    statusPanelClass(item.tone),
  )}
>
  <div class="flex items-start justify-between gap-3">
    <div class="min-w-0">
      <p class="flex items-center gap-2 text-xs uppercase tracking-[0.18em] text-muted-foreground">
        <span class={cn("h-1.5 w-1.5 rounded-full", statusDotClass(item.tone))}></span>
        {item.label}
      </p>
      <p class="mt-1 text-lg font-semibold text-foreground">{item.value}</p>
    </div>
    <div class={cn("flex h-9 w-9 shrink-0 items-center justify-center border border-border/70 bg-card/80", statusIconClass(item.tone))}>
      {#if item.id === "server"}
        <Activity class="h-4 w-4" />
      {:else if item.id === "git"}
        <GitCommit class="h-4 w-4" />
      {:else}
        <GitBranch class="h-4 w-4" />
      {/if}
    </div>
  </div>
  <p class="mt-2 text-xs leading-5 text-muted-foreground">{item.detail}</p>

  {#if item.checks}
    <HealthChecksPopover checks={item.checks} detail={item.detail} />
  {/if}
</div>
