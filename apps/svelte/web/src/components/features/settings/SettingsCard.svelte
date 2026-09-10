<script lang="ts">
  import type { Snippet } from "svelte";

  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { cn } from "$lib/utils";

  const {
    title,
    description,
    children,
    class: className = "",
    contentClass = "space-y-3",
  } = $props<{
    title: string;
    description: string;
    children?: Snippet;
    class?: string;
    contentClass?: string;
  }>();
</script>

<Card size="sm" class={cn("overflow-visible", className)}>
  <CardHeader class="pb-3">
    <CardTitle>{title}</CardTitle>
    <CardDescription>{description}</CardDescription>
  </CardHeader>
  <CardContent class={contentClass}>
    {#if children}
      {@render children()}
    {/if}
  </CardContent>
</Card>
