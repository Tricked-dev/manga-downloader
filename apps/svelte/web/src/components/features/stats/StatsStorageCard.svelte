<script lang="ts">
  import type { StatsStorageSummary } from "@manga-server/api-client/generated/model";
  import { HardDrive } from "@lucide/svelte";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";

  let {
    formatBytes,
    formatPercent,
    storage,
  } = $props<{
    formatBytes: (value?: number | null) => string;
    formatPercent: (value: number) => string;
    storage: StatsStorageSummary;
  }>();

  const hasLimit = $derived(typeof storage.limit_bytes === "number" && storage.limit_bytes > 0);
  const usageRate = $derived(Math.min(Math.max(storage.usage_rate, 0), 1));
  const progressWidth = $derived(`${Math.round(usageRate * 100)}%`);
</script>

<Card>
  <CardHeader>
    <div class="flex items-center justify-between gap-3">
      <div>
        <CardTitle class="text-base">{$t("app.stats.fileUsage")}</CardTitle>
        <CardDescription>{$t("app.stats.downloadedFiles")}</CardDescription>
      </div>
      <HardDrive class="size-5 text-muted-foreground" />
    </div>
  </CardHeader>
  <CardContent class="space-y-4">
    {#if storage.available}
      <div class="flex items-end justify-between gap-3">
        <div>
          <div class="text-3xl font-semibold">{formatBytes(storage.used_bytes)}</div>
          <p class="mt-1 text-xs text-muted-foreground">
            {#if hasLimit}
              {$t("app.stats.storageUsedOf", { limit: formatBytes(storage.limit_bytes) })}
            {:else}
              {$t("app.stats.storageNoLimit")}
            {/if}
          </p>
        </div>
        {#if hasLimit}
          <div class="shrink-0 text-right">
            <p class="text-sm font-medium">{formatPercent(usageRate)}</p>
            <p class="text-xs text-muted-foreground">{$t("app.stats.used")}</p>
          </div>
        {/if}
      </div>

      {#if hasLimit}
        <div
          class="h-2 overflow-hidden bg-muted"
          role="progressbar"
          aria-label={$t("app.stats.fileUsage")}
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={Math.round(usageRate * 100)}
        >
          <div class="h-full bg-primary transition-[width]" style:width={progressWidth}></div>
        </div>
      {/if}
    {:else}
      <div>
        <div class="text-3xl font-semibold">{formatBytes(storage.used_bytes)}</div>
        <p class="mt-1 text-xs text-muted-foreground">{$t("app.stats.storageUnavailable")}</p>
      </div>
      {#if hasLimit}
        <p class="text-xs text-muted-foreground">
          {$t("app.stats.storageCap", { limit: formatBytes(storage.limit_bytes) })}
        </p>
      {/if}
    {/if}
  </CardContent>
</Card>
