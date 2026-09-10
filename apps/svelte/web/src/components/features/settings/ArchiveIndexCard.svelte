<script lang="ts">
  import { Archive, Database, RefreshCw, Trash2, Wrench } from "@lucide/svelte";

  import type { ArchiveIndexStatusResponse } from "@manga-server/api-client/generated/model";
  import { Button } from "$lib/ui/button";
  import { Skeleton } from "$lib/ui/skeleton";
  import { t } from "$lib/i18n";
  import SettingsCard from "./SettingsCard.svelte";

  const numberFormatter = new Intl.NumberFormat("en-US");
  const byteFormatter = new Intl.NumberFormat("en-US", {
    maximumFractionDigits: 1,
  });

  let {
    status,
    loading,
    rebuilding,
    rebuildResult,
    onRebuild,
    cleaningStale,
    cleanupResult,
    onCleanupStale,
    clearingIndex,
    clearResult,
    onClearIndex,
  } = $props<{
    status: ArchiveIndexStatusResponse | undefined;
    loading: boolean;
    rebuilding: boolean;
    rebuildResult: string;
    onRebuild: () => void;
    cleaningStale: boolean;
    cleanupResult: string;
    onCleanupStale: () => void;
    clearingIndex: boolean;
    clearResult: string;
    onClearIndex: () => void;
  }>();

  const actionBusy = $derived(rebuilding || cleaningStale || clearingIndex);
  const currentResult = $derived(rebuildResult || cleanupResult || clearResult || status?.job.message || "");
  const stats = $derived([
    {
      label: $t("app.settings.archiveIndex.archives"),
      value: formatNumber(status?.indexed_archives),
      icon: Archive,
    },
    {
      label: $t("app.settings.archiveIndex.pages"),
      value: formatNumber(status?.indexed_pages),
      icon: Database,
    },
    {
      label: $t("app.settings.archiveIndex.staleRows"),
      value: formatNumber(status?.stale_rows),
      icon: Wrench,
    },
    {
      label: $t("app.settings.archiveIndex.blobBytes"),
      value: formatBytes(status?.index_blob_bytes),
      icon: Database,
    },
  ]);

  function formatNumber(value = 0) {
    return numberFormatter.format(value);
  }

  function formatBytes(value = 0) {
    if (value < 1024) {
      return `${numberFormatter.format(value)} B`;
    }
    const units = ["KiB", "MiB", "GiB"] as const;
    let amount = value / 1024;
    let unit: (typeof units)[number] = units[0];
    for (const nextUnit of units.slice(1)) {
      if (amount < 1024) {
        break;
      }
      amount /= 1024;
      unit = nextUnit;
    }
    return `${byteFormatter.format(amount)} ${unit}`;
  }
</script>

<SettingsCard
  title={$t("app.settings.archiveIndex.title")}
  description={$t("app.settings.archiveIndex.description")}
>
  {#if loading}
    <div class="grid gap-2 sm:grid-cols-2">
      {#each Array.from({ length: 4 }) as _, index (index)}
        <div class="rounded-md border border-border/70 p-3">
          <Skeleton class="h-4 w-24" />
          <Skeleton class="mt-3 h-6 w-16" />
        </div>
      {/each}
    </div>
  {:else}
    <div class="grid gap-2 sm:grid-cols-2">
      {#each stats as stat (stat.label)}
        {@const Icon = stat.icon}
        <div class="rounded-md border border-border/70 bg-background/40 p-3">
          <div class="flex items-center gap-2 text-xs text-muted-foreground">
            <Icon class="h-3.5 w-3.5" />
            <span>{stat.label}</span>
          </div>
          <p class="mt-2 text-xl font-semibold tracking-normal">{stat.value}</p>
        </div>
      {/each}
    </div>

    <div class="flex flex-wrap items-center gap-2 pt-0.5">
      <Button
        id="rebuild-archive-index-button"
        variant="secondary"
        onclick={onRebuild}
        disabled={actionBusy}
        class="h-9"
      >
        {#if rebuilding}
          {$t("app.settings.archiveIndex.rebuilding")}
        {:else}
          <RefreshCw class="mr-2 h-4 w-4" />
          {$t("app.settings.archiveIndex.rebuild")}
        {/if}
      </Button>
      <Button
        id="cleanup-archive-index-button"
        variant="secondary"
        onclick={onCleanupStale}
        disabled={actionBusy}
        class="h-9"
      >
        {#if cleaningStale}
          {$t("app.settings.archiveIndex.cleaning")}
        {:else}
          <Wrench class="mr-2 h-4 w-4" />
          {$t("app.settings.archiveIndex.cleanupStale")}
        {/if}
      </Button>
      <Button
        id="clear-archive-index-button"
        variant="secondary"
        onclick={onClearIndex}
        disabled={actionBusy}
        class="h-9"
      >
        {#if clearingIndex}
          {$t("app.settings.archiveIndex.clearing")}
        {:else}
          <Trash2 class="mr-2 h-4 w-4" />
          {$t("app.settings.archiveIndex.clear")}
        {/if}
      </Button>
    </div>

    {#if currentResult}
      <p class="text-sm text-muted-foreground">{currentResult}</p>
    {/if}
  {/if}
</SettingsCard>
