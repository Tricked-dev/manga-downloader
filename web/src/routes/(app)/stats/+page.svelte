<script lang="ts">
  import { browser } from "$app/environment";
  import { createGetStatsOverview } from "@manga-server/api-client/generated";
  import type { StatsSourceBreakdown } from "@manga-server/api-client/generated/model";
  import type { ChartConfig } from "$lib/ui/chart";
  import { Alert, AlertDescription, AlertTitle } from "$lib/ui/alert";
  import PageHeader from "@manga-server/ui/components/page-header";
  import StatsActivityCardPlaceholder from "$components/features/stats/StatsActivityCardPlaceholder.svelte";
  import type StatsActivityCardComponent from "$components/features/stats/StatsActivityCard.svelte";
  import StatsCacheCard from "$components/features/stats/StatsCacheCard.svelte";
  import StatsLoadingSkeleton from "$components/features/stats/StatsLoadingSkeleton.svelte";
  import StatsRecentReadsCard from "$components/features/stats/StatsRecentReadsCard.svelte";
  import StatsSourcesCard from "$components/features/stats/StatsSourcesCard.svelte";
  import StatsSeriesStorageCard from "$components/features/stats/StatsSeriesStorageCard.svelte";
  import StatsStorageCard from "$components/features/stats/StatsStorageCard.svelte";
  import StatsSummaryCards from "$components/features/stats/StatsSummaryCards.svelte";
  import type { ActivityDatum } from "$lib/features/stats/types";
  import { t } from "$lib/i18n";
  import { QUERY_CACHE_TIMES } from "$lib/query-client";
  import { getErrorMessage } from "$lib/utils";

  const dayFormatter = new Intl.DateTimeFormat("en-US", {
    day: "numeric",
    month: "short",
    timeZone: "UTC",
  });
  const numberFormatter = new Intl.NumberFormat("en-US");
  const compactFormatter = new Intl.NumberFormat("en-US", {
    maximumFractionDigits: 1,
    notation: "compact",
  });
  const byteFormatter = new Intl.NumberFormat("en-US", {
    maximumFractionDigits: 1,
  });
  const BYTE_UNITS = ["B", "KiB", "MiB", "GiB", "TiB"] as const;
  const MAX_X_AXIS_TICKS = 5;
  const EMPTY_ACTIVITY: Array<{ day: number; chapters_read: number; chapters_downloaded: number }> = [];

  const statsQuery = createGetStatsOverview(() => ({
    query: {
      staleTime: QUERY_CACHE_TIMES.active,
    },
  }));

  const stats = $derived(statsQuery.data);
  const totals = $derived(stats?.totals);
  const loading = $derived(statsQuery.isPending);
  const error = $derived(statsQuery.error ? getErrorMessage(statsQuery.error, $t("app.stats.requestFailed")) : "");
  const activityData: ActivityDatum[] = $derived(getActivityData());
  const hasActivity = $derived(activityData.some((point) => point.chaptersRead > 0 || point.chaptersDownloaded > 0));
  const activityDateTicks: string[] = $derived(buildDateTicks(activityData));
  const cacheHitRate = $derived(stats?.cache.hit_rate ?? 0);
  const cacheHitRatePercent = $derived(Math.round(clampPercent(cacheHitRate) * 100));
  const cacheChartBackground = $derived(
    `conic-gradient(var(--chart-1) ${cacheHitRatePercent}%, color-mix(in oklch, var(--border) 62%, transparent) 0)`,
  );
  const topSources: StatsSourceBreakdown[] = $derived(getTopSources());
  const statsActivityCardModule = browser
    ? import("$components/features/stats/StatsActivityCard.svelte") as Promise<{
        default: typeof StatsActivityCardComponent;
      }>
    : null;

  const activityChartConfig = {
    chaptersRead: {
      label: $t("app.stats.chaptersRead"),
      color: "var(--chart-1)",
    },
    chaptersDownloaded: {
      label: $t("app.stats.chaptersDownloaded"),
      color: "oklch(0.66 0.17 250)",
    },
  } satisfies ChartConfig;

  function formatDay(day: number) {
    return dayFormatter.format(new Date(day * 86_400_000));
  }

  function getActivityData(): ActivityDatum[] {
    return (stats?.activity ?? EMPTY_ACTIVITY).map((point) => ({
      day: point.day,
      label: formatDay(point.day),
      chaptersRead: point.chapters_read,
      chaptersDownloaded: point.chapters_downloaded,
    }));
  }

  function getTopSources(): StatsSourceBreakdown[] {
    return [...(stats?.source_breakdown ?? [])]
      .sort((left, right) =>
        (right.chapters_read + right.chapters_downloaded) - (left.chapters_read + left.chapters_downloaded)
      )
      .slice(0, 8);
  }

  function formatNumber(value = 0) {
    return numberFormatter.format(value);
  }

  function formatBytes(value: number | null | undefined = 0) {
    let bytes = typeof value === "number" && Number.isFinite(value) ? Math.max(value, 0) : 0;
    let unitIndex = 0;

    while (bytes >= 1024 && unitIndex < BYTE_UNITS.length - 1) {
      bytes /= 1024;
      unitIndex += 1;
    }

    const formatted = unitIndex === 0 ? numberFormatter.format(Math.round(bytes)) : byteFormatter.format(bytes);
    return `${formatted} ${BYTE_UNITS[unitIndex]}`;
  }

  function formatCompact(value: unknown) {
    const number = typeof value === "number" ? value : Number(value);
    return Number.isFinite(number) ? compactFormatter.format(number) : "";
  }

  function formatPercent(value: number) {
    return `${Math.round(clampPercent(value) * 100)}%`;
  }

  function clampPercent(value: number) {
    return Math.min(Math.max(value, 0), 1);
  }

  function buildDateTicks(data: ActivityDatum[]) {
    if (data.length <= MAX_X_AXIS_TICKS) {
      return data.map((point) => point.label);
    }

    const lastIndex = data.length - 1;
    const step = lastIndex / (MAX_X_AXIS_TICKS - 1);
    return Array.from({ length: MAX_X_AXIS_TICKS }, (_, index) => data[Math.round(index * step)].label);
  }
</script>

<svelte:head>
  <title>{$t("app.stats.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<div class="space-y-6">
  <PageHeader
    title={$t("app.stats.title")}
    description={$t("app.stats.description")}
  />

  {#if error}
    <Alert variant="destructive">
      <AlertTitle>{$t("app.stats.unavailable")}</AlertTitle>
      <AlertDescription>{error}</AlertDescription>
    </Alert>
  {/if}

  {#if loading}
    <StatsLoadingSkeleton />
  {:else if stats && totals}
    <StatsSummaryCards {totals} {formatNumber} />

    <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
      {#if statsActivityCardModule}
        {#await statsActivityCardModule}
          <StatsActivityCardPlaceholder {hasActivity} />
        {:then { default: StatsActivityCard }}
          <StatsActivityCard
            {activityChartConfig}
            {activityData}
            {activityDateTicks}
            {formatCompact}
            {hasActivity}
          />
        {:catch}
          <StatsActivityCardPlaceholder {hasActivity} />
        {/await}
      {:else}
        <StatsActivityCardPlaceholder {hasActivity} />
      {/if}

      <div class="space-y-4">
        <StatsCacheCard
          cache={stats.cache}
          {cacheChartBackground}
          {cacheHitRate}
          {formatNumber}
          {formatPercent}
        />
        <StatsStorageCard
          {formatBytes}
          {formatPercent}
          storage={stats.storage}
        />
        <StatsSeriesStorageCard
          {formatBytes}
          {formatNumber}
          recordedBytes={stats.recorded_bytes}
          series={stats.series_storage}
        />
        <StatsSourcesCard
          formatNumber={formatNumber}
          pluginSources={totals.plugin_sources}
          sources={topSources}
        />
      </div>
    </div>

    <StatsRecentReadsCard chapters={stats.recent_reads} />
  {/if}
</div>
