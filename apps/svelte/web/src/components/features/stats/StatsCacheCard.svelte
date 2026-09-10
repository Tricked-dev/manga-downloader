<script lang="ts">
  import type { StatsCacheSummary } from "@manga-server/api-client/generated/model";
  import { Gauge } from "@lucide/svelte";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";

  let {
    cache,
    cacheChartBackground,
    cacheHitRate,
    formatNumber,
    formatPercent,
  } = $props<{
    cache: StatsCacheSummary;
    cacheChartBackground: string;
    cacheHitRate: number;
    formatNumber: (value?: number) => string;
    formatPercent: (value: number) => string;
  }>();
</script>

<Card>
  <CardHeader>
    <div class="flex items-center justify-between gap-3">
      <div>
        <CardTitle class="text-base">{$t("app.stats.cacheRate")}</CardTitle>
        <CardDescription>{$t("app.stats.sinceServerStart")}</CardDescription>
      </div>
      <Gauge class="size-5 text-muted-foreground" />
    </div>
  </CardHeader>
  <CardContent>
    <div class="flex flex-col items-center gap-4">
      <div
        class="relative grid size-40 place-items-center rounded-full bg-[image:var(--cache-chart-background)]"
        style:--cache-chart-background={cacheChartBackground}
        role="img"
        aria-label={$t("app.stats.cacheHitRate", { rate: formatPercent(cacheHitRate) })}
      >
        <div class="grid size-28 place-items-center rounded-full bg-card text-center">
          <div>
            <div class="text-3xl font-semibold">{formatPercent(cacheHitRate)}</div>
            <p class="mt-1 text-xs text-muted-foreground">{$t("app.stats.hitRate")}</p>
          </div>
        </div>
      </div>
      <div class="grid w-full grid-cols-3 gap-2 text-center text-xs">
        <div class="border border-border/70 px-2 py-2">
          <p class="font-medium">{formatNumber(cache.requests)}</p>
          <p class="mt-1 text-muted-foreground">{$t("app.stats.reads")}</p>
        </div>
        <div class="border border-border/70 px-2 py-2">
          <p class="font-medium">{formatNumber(cache.hits)}</p>
          <p class="mt-1 text-muted-foreground">{$t("app.stats.hits")}</p>
        </div>
        <div class="border border-border/70 px-2 py-2">
          <p class="font-medium">{formatNumber(cache.misses)}</p>
          <p class="mt-1 text-muted-foreground">{$t("app.stats.misses")}</p>
        </div>
      </div>
    </div>
  </CardContent>
</Card>
