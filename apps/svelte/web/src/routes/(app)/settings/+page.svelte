<script lang="ts">
  import { browser } from "$app/environment";
  import { Check } from "@lucide/svelte";
  import { untrack } from "svelte";
  import type SettingsPanelsComponent from "$components/features/settings/SettingsPanels.svelte";

  import {
    createCleanupStaleArchiveIndexes,
    createClearArchiveIndex,
    createClearCache,
    createCleanupDatabase,
    createGetSettings,
    createGetArchiveIndexStatus,
    createListSources,
    createRebuildArchiveIndex,
    createReencodeDownloadedAvif,
    createTriggerLibraryUpdate,
    createUpdateSettings,
    getGetArchiveIndexStatusQueryKey,
    getGetSettingsQueryKey,
    getListSourcesQueryKey,
  } from "@manga-server/api-client/generated";
  import { useHydrate, useQueryClient, type DehydratedState } from "@tanstack/svelte-query";
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";
  import { Skeleton } from "$lib/ui/skeleton";
  import PageHeader from "@manga-server/ui/components/page-header";
  import SettingsLoadingSkeleton from "$components/features/settings/SettingsLoadingSkeleton.svelte";
  import { t } from "$lib/i18n";
  import { DEFAULT_SETTINGS } from "$lib/settings";
  import {
    buildChangedSettingsPayload,
    buildSettingsSaveSnapshot,
    getAvifConversionWorkers,
    getSourceAvifEnabled,
    getSourceAvifQuality,
    hasChangedSettings,
    sourceAvifEnabledKey,
    sourceAvifQualityKey,
  } from "$lib/features/settings/utils";
  import { QUERY_CACHE_TIMES } from "$lib/query-client";
  import { dehydratedQueryOptions } from "$lib/query-hydration";
  import { selectItems } from "$lib/query-selectors";
  import type { SettingsMap } from "$lib/features/settings/types";
  import { getErrorMessage, parseCsvSetting } from "$lib/utils";
  import type { Source } from "$lib/types";

  const EMPTY_SOURCES: Source[] = [];
  const EMPTY_CATEGORIES: string[] = [];

  const { data } = $props<{
    data: {
      dehydratedState?: DehydratedState;
    };
  }>();

  let SettingsPanels = $state<typeof SettingsPanelsComponent | null>(null);
  let settings = $state<SettingsMap>(createSettingsDraft(undefined));
  let settingsInitialized = $state(false);
  const queryClient = useQueryClient();

  useHydrate(untrack(() => data.dehydratedState), undefined, queryClient);

  if (browser) {
    void import("$components/features/settings/SettingsPanels.svelte").then((module) => {
      SettingsPanels = module.default;
    });
  }

  const settingsQuery = createGetSettings(() => ({
    query: {
      ...dehydratedQueryOptions(data.dehydratedState, getGetSettingsQueryKey()),
      staleTime: QUERY_CACHE_TIMES.stable,
    },
  }));

  const sourcesQuery = createListSources(() => ({
    query: {
      ...dehydratedQueryOptions(data.dehydratedState, getListSourcesQueryKey()),
      select: selectItems<Source>,
      staleTime: QUERY_CACHE_TIMES.stable,
    },
  }));
  const archiveIndexQuery = createGetArchiveIndexStatus(() => ({
    query: {
      ...dehydratedQueryOptions(data.dehydratedState, getGetArchiveIndexStatusQueryKey()),
      staleTime: QUERY_CACHE_TIMES.live,
    },
  }));

  const loading = $derived(settingsQuery.isPending);
  let saving = $state(false);
  let saved = $state(false);
  let saveError = $state("");
  let triggeringUpdate = $state(false);
  let updateResult = $state("");
  let reencoding = $state(false);
  let reencodeResult = $state("");
  let clearingCache = $state(false);
  let clearCacheResult = $state("");
  let cleaningDatabase = $state(false);
  let cleanupDatabaseResult = $state("");
  let rebuildingArchiveIndex = $state(false);
  let archiveIndexRebuildResult = $state("");
  let cleaningArchiveIndexStale = $state(false);
  let archiveIndexCleanupResult = $state("");
  let clearingArchiveIndex = $state(false);
  let archiveIndexClearResult = $state("");

  // Derived state for shadcn components bounds
  const autoDownload = $derived(settings.auto_download_new_chapters === 'true');
  const sources = $derived(sourcesQuery.data ?? EMPTY_SOURCES);
  const sourcesLoading = $derived(sourcesQuery.isPending && sources.length === 0);
  const libraryCategories = $derived(parseCsvSetting(settings.library_categories, EMPTY_CATEGORIES));
  const imageProcessingSkeletonRows = Array.from({ length: 2 }, (_, index) => index);

  const saveSettingsMutation = createUpdateSettings();

  const updateMutation = createTriggerLibraryUpdate();

  const reencodeMutation = createReencodeDownloadedAvif();

  const clearCacheMutation = createClearCache();
  const cleanupDatabaseMutation = createCleanupDatabase();
  const rebuildArchiveIndexMutation = createRebuildArchiveIndex();
  const cleanupStaleArchiveIndexesMutation = createCleanupStaleArchiveIndexes();
  const clearArchiveIndexMutation = createClearArchiveIndex();

  function applySettingsSnapshot(nextSettings: SettingsMap | undefined) {
    settings = createSettingsDraft(nextSettings);
  }

  function createSettingsDraft(incoming: SettingsMap | undefined): SettingsMap {
    return { ...DEFAULT_SETTINGS, ...incoming };
  }

  $effect(() => {
    const nextSettings = settingsQuery.data?.settings;
    if (!settingsInitialized && nextSettings) {
      applySettingsSnapshot(nextSettings);
      settingsInitialized = true;
    }
  });

  function setSourceAvifEnabled(sourceName: string, enabled: boolean) {
    settings[sourceAvifEnabledKey(sourceName)] = enabled ? "true" : "false";
  }

  function setAvifConversionWorkers(workers: number) {
    const normalized = Number.isFinite(workers) && workers > 0 ? Math.min(Math.floor(workers), 32) : 1;
    settings.avif_conversion_workers = normalized.toString();
  }

  function setSourceAvifQuality(sourceName: string, quality: number) {
    settings[sourceAvifQualityKey(sourceName)] = quality.toString();
  }

  function avifConversionWorkers(): number {
    return getAvifConversionWorkers(settings);
  }

  function sourceEnabled(sourceName: string): boolean {
    return getSourceAvifEnabled(settings, sourceName);
  }

  function sourceQuality(sourceName: string): number {
    return getSourceAvifQuality(settings, sourceName);
  }

  function errorMessage(error: unknown) {
    return getErrorMessage(error, $t("app.errors.genericShort"));
  }

  async function saveSettings() {
    saving = true;
    saved = false;
    saveError = "";
    try {
      const sourceNames = sources.map((source: Source) => source.name);
      const nextSettings = buildSettingsSaveSnapshot(settings, sourceNames);
      const previousSettings = buildSettingsSaveSnapshot(
        createSettingsDraft(settingsQuery.data?.settings),
        sourceNames,
      );
      const payload = buildChangedSettingsPayload(nextSettings, previousSettings);

      if (hasChangedSettings(payload)) {
        await saveSettingsMutation.mutateAsync({ data: payload });
      }
      queryClient.setQueryData(getGetSettingsQueryKey(), {
        settings: nextSettings,
      });
      applySettingsSnapshot(nextSettings);
      saved = true;
      setTimeout(() => {
        saved = false;
      }, 3000);
    } catch (error: unknown) {
      saveError = errorMessage(error);
    } finally {
      saving = false;
    }
  }

  async function triggerUpdate() {
    triggeringUpdate = true;
    updateResult = "";
    try {
      const res = await updateMutation.mutateAsync();
      updateResult = $t("app.settings.libraryUpdateResult", { count: res.new_chapters });
    } catch (error: unknown) {
      updateResult = errorMessage(error);
    } finally {
      triggeringUpdate = false;
    }
  }

  async function reencodeDownloadedAvif() {
    reencoding = true;
    reencodeResult = "";
    try {
      const res = await reencodeMutation.mutateAsync() as {
        files_processed: number;
        images_reencoded: number;
      };
      reencodeResult = $t("app.settings.avif.reencodeResult", {
        archives: res.files_processed,
        images: res.images_reencoded,
      });
    } catch (error: unknown) {
      reencodeResult = errorMessage(error);
    } finally {
      reencoding = false;
    }
  }

  async function clearFoyerCache() {
    clearingCache = true;
    clearCacheResult = "";
    cleanupDatabaseResult = "";
    try {
      await clearCacheMutation.mutateAsync();
      clearCacheResult = $t("app.settings.cache.clearResult");
    } catch (error: unknown) {
      clearCacheResult = errorMessage(error);
    } finally {
      clearingCache = false;
    }
  }

  async function cleanupDatabase() {
    cleaningDatabase = true;
    cleanupDatabaseResult = "";
    clearCacheResult = "";
    try {
      await cleanupDatabaseMutation.mutateAsync();
      cleanupDatabaseResult = $t("app.settings.cache.cleanupResult");
    } catch (error: unknown) {
      cleanupDatabaseResult = errorMessage(error);
    } finally {
      cleaningDatabase = false;
    }
  }

  async function rebuildArchiveIndex() {
    rebuildingArchiveIndex = true;
    archiveIndexRebuildResult = "";
    archiveIndexCleanupResult = "";
    archiveIndexClearResult = "";
    try {
      const res = await rebuildArchiveIndexMutation.mutateAsync();
      archiveIndexRebuildResult = res.message;
      await queryClient.invalidateQueries({ queryKey: getGetArchiveIndexStatusQueryKey() });
    } catch (error: unknown) {
      archiveIndexRebuildResult = errorMessage(error);
    } finally {
      rebuildingArchiveIndex = false;
    }
  }

  async function cleanupArchiveIndexStale() {
    cleaningArchiveIndexStale = true;
    archiveIndexRebuildResult = "";
    archiveIndexCleanupResult = "";
    archiveIndexClearResult = "";
    try {
      const res = await cleanupStaleArchiveIndexesMutation.mutateAsync();
      archiveIndexCleanupResult = $t("app.settings.archiveIndex.cleanupResult", { count: res.removed_rows });
      await queryClient.invalidateQueries({ queryKey: getGetArchiveIndexStatusQueryKey() });
    } catch (error: unknown) {
      archiveIndexCleanupResult = errorMessage(error);
    } finally {
      cleaningArchiveIndexStale = false;
    }
  }

  async function clearArchiveIndex() {
    clearingArchiveIndex = true;
    archiveIndexRebuildResult = "";
    archiveIndexCleanupResult = "";
    archiveIndexClearResult = "";
    try {
      const res = await clearArchiveIndexMutation.mutateAsync();
      archiveIndexClearResult = $t("app.settings.archiveIndex.clearResult", { count: res.removed_rows });
      await queryClient.invalidateQueries({ queryKey: getGetArchiveIndexStatusQueryKey() });
    } catch (error: unknown) {
      archiveIndexClearResult = errorMessage(error);
    } finally {
      clearingArchiveIndex = false;
    }
  }
</script>

<svelte:head>
  <title>{$t("app.settings.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<div class="max-w-5xl space-y-5">
  <PageHeader
    title={$t("app.settings.title")}
    description={$t("app.settings.description")}
    innerClass="lg:items-center"
  >
    {#snippet actions()}
      <div class="flex min-w-[12rem] flex-col items-end gap-1.5">
        {#if loading}
          <Skeleton class="h-9 w-32 rounded-md" />
          <Skeleton class="h-4 w-40" />
        {:else}
          <div class="flex w-full flex-wrap items-center justify-end gap-3">
            {#if saved}
              <Badge variant="success" class="gap-1">
                <Check class="mr-1 h-4 w-4" /> {$t("app.settings.saved")}
              </Badge>
            {/if}
            <Button
              size="lg"
              onclick={saveSettings}
              disabled={saving || loading || sourcesLoading}
              class="h-9 px-4"
            >
              {saving ? $t("app.settings.saving") : $t("app.settings.save")}
            </Button>
          </div>
          {#if saveError}
            <p class="text-right text-sm text-destructive">{saveError}</p>
          {/if}
        {/if}
      </div>
    {/snippet}
  </PageHeader>

  {#if loading || !SettingsPanels}
    <SettingsLoadingSkeleton />
  {:else}
    <SettingsPanels
      bind:settings
      {autoDownload}
      {libraryCategories}
      {triggeringUpdate}
      {updateResult}
      onTriggerUpdate={triggerUpdate}
      {clearingCache}
      {clearCacheResult}
      onClearCache={clearFoyerCache}
      {cleaningDatabase}
      {cleanupDatabaseResult}
      onCleanupDatabase={cleanupDatabase}
      archiveIndexStatus={archiveIndexQuery.data}
      archiveIndexLoading={archiveIndexQuery.isPending}
      {rebuildingArchiveIndex}
      {archiveIndexRebuildResult}
      onRebuildArchiveIndex={rebuildArchiveIndex}
      {cleaningArchiveIndexStale}
      {archiveIndexCleanupResult}
      onCleanupArchiveIndexStale={cleanupArchiveIndexStale}
      {clearingArchiveIndex}
      {archiveIndexClearResult}
      onClearArchiveIndex={clearArchiveIndex}
      getAvifConversionWorkers={avifConversionWorkers}
      {setAvifConversionWorkers}
      {reencoding}
      {reencodeResult}
      onReencode={reencodeDownloadedAvif}
      {sourcesLoading}
      {imageProcessingSkeletonRows}
      {sources}
      getSourceAvifEnabled={sourceEnabled}
      getSourceAvifQuality={sourceQuality}
      {setSourceAvifEnabled}
      {setSourceAvifQuality}
    />
  {/if}
</div>
