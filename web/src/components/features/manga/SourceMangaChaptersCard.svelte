<script lang="ts">
  import type { Snippet } from "svelte";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import LoadingState from "$components/common/LoadingState.svelte";
  import { t } from "$lib/i18n";

  let {
    actionMessage,
    chaptersError,
    chaptersLoading,
    count,
    sourceName,
    children,
  } = $props<{
    actionMessage: string;
    chaptersError: string;
    chaptersLoading: boolean;
    count: number;
    sourceName: string;
    children: Snippet;
  }>();
</script>

<Card class="gap-0">
  <CardHeader class="border-b border-border/70">
    <CardTitle>{$t("app.manga.chapters")}</CardTitle>
    <CardDescription>{$t("app.manga.chaptersAvailable", { count, source: sourceName })}</CardDescription>
  </CardHeader>
  <CardContent class="p-0">
    {#if actionMessage}
      <p class="border-b border-border px-5 py-3 text-sm text-destructive">{actionMessage}</p>
    {/if}
    {#if chaptersLoading}
      <div class="px-5 py-6">
        <LoadingState label={$t("app.manga.loadingChapters")} />
      </div>
    {:else if chaptersError}
      <p class="px-5 py-4 text-sm text-destructive">{chaptersError}</p>
    {:else}
      {@render children()}
    {/if}
  </CardContent>
</Card>
