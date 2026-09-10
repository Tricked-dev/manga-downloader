<script lang="ts">
  import { Button } from "$lib/ui/button";
  import type { IconComponent } from "$lib/types";

  const {
    icon: Icon,
    title,
    description = "",
    actionHref,
    actionLabel,
  } = $props<{
    icon: IconComponent;
    title: string;
    description?: string;
    actionHref?: string;
    actionLabel?: string;
  }>();
</script>

<div class="flex flex-col items-center justify-center border border-dashed border-border/70 bg-card px-6 py-16 text-center">
  <div class="mb-4 flex h-14 w-14 items-center justify-center bg-muted text-muted-foreground">
    <Icon class="h-7 w-7" />
  </div>
  <h2 class="text-lg font-semibold">{title}</h2>

  {#if description}
    <p class="mt-2 max-w-md text-sm text-muted-foreground">{description}</p>
  {/if}

  {#if actionHref && actionLabel}
    <Button href={actionHref} class="mt-6">
      {actionLabel}
    </Button>
  {/if}
</div>
