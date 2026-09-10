<script lang="ts">
  import { AreaChart } from "layerchart";
  import { ChartArea } from "@lucide/svelte";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { ChartContainer, ChartTooltip, type ChartConfig } from "$lib/ui/chart";
  import { t } from "$lib/i18n";
  import type { ActivityDatum } from "$lib/features/stats/types";

  let {
    activityChartConfig,
    activityData,
    activityDateTicks,
    formatCompact,
    hasActivity,
  } = $props<{
    activityChartConfig: ChartConfig;
    activityData: ActivityDatum[];
    activityDateTicks: string[];
    formatCompact: (value: unknown) => string;
    hasActivity: boolean;
  }>();

  const legendItems = $derived([
    {
      key: "chaptersRead",
      label: activityChartConfig.chaptersRead.label,
      color: activityChartConfig.chaptersRead.color ?? "currentColor",
    },
    {
      key: "chaptersDownloaded",
      label: activityChartConfig.chaptersDownloaded.label,
      color: activityChartConfig.chaptersDownloaded.color ?? "currentColor",
    },
  ]);
</script>

<Card class="h-full">
  <CardHeader>
    <div class="flex items-center justify-between gap-3">
      <div>
        <CardTitle class="text-base">{$t("app.stats.last30Days")}</CardTitle>
        <CardDescription>{$t("app.stats.last30DaysDescription")}</CardDescription>
      </div>
      <ChartArea class="hidden size-5 text-muted-foreground sm:block" />
    </div>
  </CardHeader>
  <CardContent class="flex flex-1 flex-col gap-3">
    {#if hasActivity}
      <ChartContainer config={activityChartConfig} class="min-h-[280px] flex-1 w-full aspect-auto">
        <AreaChart
          data={activityData}
          x="label"
          axis="x"
          seriesLayout="overlap"
          series={[
            {
              key: "chaptersRead",
              label: activityChartConfig.chaptersRead.label,
              value: "chaptersRead",
              color: activityChartConfig.chaptersRead.color,
            },
            {
              key: "chaptersDownloaded",
              label: activityChartConfig.chaptersDownloaded.label,
              value: "chaptersDownloaded",
              color: activityChartConfig.chaptersDownloaded.color,
            },
          ]}
          props={{
            xAxis: {
              format: (value) => String(value),
              ticks: activityDateTicks,
            },
            yAxis: {
              format: formatCompact,
            },
          }}
        >
          {#snippet tooltip()}
            <ChartTooltip />
          {/snippet}
        </AreaChart>
      </ChartContainer>
      <div class="mt-auto flex flex-wrap items-center justify-center gap-x-4 gap-y-2 border-t border-border/70 pt-3 text-xs text-muted-foreground">
        {#each legendItems as item (item.key)}
          <div class="flex items-center gap-2">
            <span
              class="size-2.5 shrink-0 rounded-[2px]"
              style:background-color={item.color}
              aria-hidden="true"
            ></span>
            <span>{item.label}</span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="flex min-h-[280px] flex-1 items-center justify-center border border-dashed border-border/80 bg-muted/20">
        <p class="text-sm text-muted-foreground">{$t("app.stats.noActivity")}</p>
      </div>
    {/if}
  </CardContent>
</Card>
