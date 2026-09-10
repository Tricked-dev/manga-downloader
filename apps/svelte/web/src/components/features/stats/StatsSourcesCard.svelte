<script lang="ts">
  import type { StatsSourceBreakdown } from "@manga-server/api-client/generated/model";
  import { Plug } from "@lucide/svelte";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";

  let {
    formatNumber,
    pluginSources,
    sources,
  } = $props<{
    formatNumber: (value?: number) => string;
    pluginSources: number;
    sources: StatsSourceBreakdown[];
  }>();
</script>

<Card>
  <CardHeader>
    <CardTitle class="text-base">{$t("app.stats.sources")}</CardTitle>
    <CardDescription>{$t("app.stats.installedPlugins", { count: formatNumber(pluginSources) })}</CardDescription>
  </CardHeader>
  <CardContent class="space-y-3">
    {#if sources.length > 0}
      {#each sources as source (source.source)}
        <div class="flex items-center justify-between gap-3 border-b border-border/70 pb-3 last:border-0 last:pb-0">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <Plug class="size-3.5 shrink-0 text-muted-foreground" />
              <p class="truncate text-sm font-medium">{source.source}</p>
            </div>
            <p class="mt-1 text-xs text-muted-foreground">
              {$t("app.stats.seriesAndChapters", { series: formatNumber(source.series), chapters: formatNumber(source.chapters) })}
            </p>
          </div>
          <div class="shrink-0 text-right">
            <p class="text-sm font-medium">{formatNumber(source.chapters_read + source.chapters_downloaded)}</p>
            <p class="text-xs text-muted-foreground">{$t("app.stats.chapters")}</p>
          </div>
        </div>
      {/each}
    {:else}
      <p class="text-sm text-muted-foreground">{$t("app.stats.noSources")}</p>
    {/if}
  </CardContent>
</Card>
