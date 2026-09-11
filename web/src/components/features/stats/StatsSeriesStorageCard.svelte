<script lang="ts">
  import type { StatsSeriesStorage } from "@manga-server/api-client/generated/model";
  import { Library } from "@lucide/svelte";
  import { Badge } from "$lib/ui/badge";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";

  const {
    formatBytes,
    formatNumber,
    recordedBytes,
    series,
  } = $props<{
    formatBytes: (value?: number | null) => string;
    formatNumber: (value: number) => string;
    recordedBytes: number;
    series: StatsSeriesStorage[];
  }>();

  // The list is already largest-first, and a long tail of small series buries the answer
  // the card exists to give: what is taking the space.
  const VISIBLE_SERIES = 12;

  const visible = $derived(series.slice(0, VISIBLE_SERIES));
  const remaining = $derived(series.length - visible.length);
  const remainingBytes = $derived(
    series
      .slice(VISIBLE_SERIES)
      .reduce((total: number, entry: StatsSeriesStorage) => total + entry.bytes, 0),
  );
  const largestBytes = $derived(series[0]?.bytes ?? 0);

  function share(bytes: number): string {
    return largestBytes > 0 ? `${Math.round((bytes / largestBytes) * 100)}%` : "0%";
  }
</script>

<Card>
  <CardHeader>
    <div class="flex items-center justify-between gap-3">
      <div>
        <CardTitle class="text-base">{$t("app.stats.seriesStorage")}</CardTitle>
        <CardDescription>{$t("app.stats.seriesStorageDescription")}</CardDescription>
      </div>
      <Library class="size-5 text-muted-foreground" />
    </div>
  </CardHeader>
  <CardContent class="space-y-4">
    <div>
      <div class="text-3xl font-semibold">{formatBytes(recordedBytes)}</div>
      <p class="mt-1 text-xs text-muted-foreground">
        {$t("app.stats.seriesStorageTotal", { count: series.length })}
      </p>
    </div>

    {#if series.length === 0}
      <p class="text-sm text-muted-foreground">{$t("app.stats.seriesStorageEmpty")}</p>
    {:else}
      <ul class="space-y-3">
        {#each visible as entry (entry.series_id)}
          <li class="space-y-1.5">
            <div class="flex items-baseline justify-between gap-3">
              <span class="truncate text-sm font-medium">{entry.title}</span>
              <span class="shrink-0 text-sm tabular-nums">{formatBytes(entry.bytes)}</span>
            </div>
            <div class="h-1.5 overflow-hidden rounded-full bg-muted">
              <div class="h-full bg-primary" style:width={share(entry.bytes)}></div>
            </div>
            <div class="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <span>{$t("app.stats.seriesStorageChapters", { count: formatNumber(entry.chapters) })}</span>
              {#if entry.upscaled_chapters > 0}
                <Badge variant="muted">
                  {$t("app.stats.seriesStorageUpscaled", { count: formatNumber(entry.upscaled_chapters) })}
                </Badge>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
      {#if remaining > 0}
        <p class="text-xs text-muted-foreground">
          {$t("app.stats.seriesStorageRemaining", {
            count: formatNumber(remaining),
            size: formatBytes(remainingBytes),
          })}
        </p>
      {/if}
    {/if}
  </CardContent>
</Card>
