<script lang="ts">
  import { ArrowLeft } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import { cn } from "../utils";

  let {
    title,
    description = "",
    eyebrow = "",
    backHref,
    backLabel = "Back",
    actions,
    class: className = "",
    innerClass = "",
    eyebrowClass = "",
    titleClass = "",
    descriptionClass = "",
    actionsClass = "",
  }: {
    actions?: Snippet;
    actionsClass?: string;
    backHref?: string;
    backLabel?: string;
    class?: string;
    description?: string;
    descriptionClass?: string;
    eyebrow?: string;
    eyebrowClass?: string;
    innerClass?: string;
    title: string;
    titleClass?: string;
  } = $props();
</script>

<section class={cn("border border-border/80 bg-card px-4 py-4 sm:px-5", className)}>
  <div class={cn("flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between", innerClass)}>
    <div class="min-w-0">
      {#if eyebrow}
        <p class={cn("muted-label mb-1", eyebrowClass)}>{eyebrow}</p>
      {/if}

      {#if backHref}
        <a
          href={backHref}
          class="group mb-2 -ml-2 inline-flex h-8 items-center gap-1 rounded-lg px-2.5 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        >
          <ArrowLeft class="size-4" />
          {backLabel}
        </a>
      {/if}

      <h1 class={cn("text-2xl font-semibold tracking-tight text-foreground", titleClass)}>{title}</h1>

      {#if description}
        <p class={cn("mt-1 text-sm text-muted-foreground", descriptionClass)}>{description}</p>
      {/if}
    </div>

    {#if actions}
      <div class={cn("flex flex-col gap-3 sm:items-end", actionsClass)}>
        {@render actions()}
      </div>
    {/if}
  </div>
</section>
