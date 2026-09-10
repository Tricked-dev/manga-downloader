<script lang="ts">
  import { BookOpen, Database, Image, LayoutGrid, Plug } from "@lucide/svelte";

  import AvifSettingsCard from "$components/features/settings/AvifSettingsCard.svelte";
  import ArchiveIndexCard from "$components/features/settings/ArchiveIndexCard.svelte";
  import AuthCard from "$components/features/settings/AuthCard.svelte";
  import BackendApiCard from "$components/features/settings/BackendApiCard.svelte";
  import CacheCard from "$components/features/settings/CacheCard.svelte";
  import DiscoveryCard from "$components/features/settings/DiscoveryCard.svelte";
  import DiscordCard from "$components/features/settings/DiscordCard.svelte";
  import DownloadConcurrencyCard from "$components/features/settings/DownloadConcurrencyCard.svelte";
  import ImageProcessingCard from "$components/features/settings/ImageProcessingCard.svelte";
  import LibraryUpdatesCard from "$components/features/settings/LibraryUpdatesCard.svelte";
  import StorageCard from "$components/features/settings/StorageCard.svelte";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { Skeleton } from "$lib/ui/skeleton";
  import { Tabs, TabsContent, TabsList, TabsTrigger } from "$lib/ui/tabs";
  import { t } from "$lib/i18n";
  import type { SettingsMap } from "$lib/features/settings/types";
  import type { Source } from "$lib/types";
  import type { ArchiveIndexStatusResponse } from "@manga-server/api-client/generated/model";

  let {
    settings = $bindable(),
    autoDownload,
    libraryCategories,
    triggeringUpdate,
    updateResult,
    onTriggerUpdate,
    clearingCache,
    clearCacheResult,
    onClearCache,
    cleaningDatabase,
    cleanupDatabaseResult,
    onCleanupDatabase,
    archiveIndexStatus,
    archiveIndexLoading,
    rebuildingArchiveIndex,
    archiveIndexRebuildResult,
    onRebuildArchiveIndex,
    cleaningArchiveIndexStale,
    archiveIndexCleanupResult,
    onCleanupArchiveIndexStale,
    clearingArchiveIndex,
    archiveIndexClearResult,
    onClearArchiveIndex,
    getAvifConversionWorkers,
    setAvifConversionWorkers,
    reencoding,
    reencodeResult,
    onReencode,
    sourcesLoading,
    imageProcessingSkeletonRows,
    sources,
    getSourceAvifEnabled,
    getSourceAvifQuality,
    setSourceAvifEnabled,
    setSourceAvifQuality,
  } = $props<{
    settings: SettingsMap;
    autoDownload: boolean;
    libraryCategories: string[];
    triggeringUpdate: boolean;
    updateResult: string;
    onTriggerUpdate: () => void;
    clearingCache: boolean;
    clearCacheResult: string;
    onClearCache: () => void;
    cleaningDatabase: boolean;
    cleanupDatabaseResult: string;
    onCleanupDatabase: () => void;
    archiveIndexStatus: ArchiveIndexStatusResponse | undefined;
    archiveIndexLoading: boolean;
    rebuildingArchiveIndex: boolean;
    archiveIndexRebuildResult: string;
    onRebuildArchiveIndex: () => void;
    cleaningArchiveIndexStale: boolean;
    archiveIndexCleanupResult: string;
    onCleanupArchiveIndexStale: () => void;
    clearingArchiveIndex: boolean;
    archiveIndexClearResult: string;
    onClearArchiveIndex: () => void;
    getAvifConversionWorkers: () => number;
    setAvifConversionWorkers: (workers: number) => void;
    reencoding: boolean;
    reencodeResult: string;
    onReencode: () => void;
    sourcesLoading: boolean;
    imageProcessingSkeletonRows: number[];
    sources: Source[];
    getSourceAvifEnabled: (sourceName: string) => boolean;
    getSourceAvifQuality: (sourceName: string) => number;
    setSourceAvifEnabled: (sourceName: string, enabled: boolean) => void;
    setSourceAvifQuality: (sourceName: string, quality: number) => void;
  }>();

  const tabTriggerClass =
    "h-8 flex-none px-3 text-muted-foreground hover:bg-muted/40 hover:text-foreground focus-visible:border-border/60 focus-visible:ring-1 focus-visible:ring-border/50 focus-visible:outline-none";
</script>

{#snippet imageProcessingSettings()}
  {#if sourcesLoading}
    <Card size="sm" class="overflow-visible">
      <CardHeader class="pb-3">
        <CardTitle>{$t("app.settings.imageProcessing.title")}</CardTitle>
        <CardDescription>{$t("app.settings.imageProcessing.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <div class="space-y-3">
          {#each imageProcessingSkeletonRows as index (index)}
            <div class="space-y-3 rounded-md border border-border/70 bg-card/50 p-3.5">
              <div class="flex items-center justify-between gap-4">
                <div class="space-y-2">
                  <Skeleton class="h-4 w-28" />
                  <Skeleton class="h-3 w-48" />
                </div>
                <Skeleton class="h-6 w-16 rounded-md" />
              </div>
              <div class="flex items-center justify-between gap-4">
                <div class="space-y-2">
                  <Skeleton class="h-4 w-32" />
                  <Skeleton class="h-3 w-44" />
                </div>
                <Skeleton class="h-6 w-10 rounded-full" />
              </div>
              <div class="space-y-2.5 rounded-md border border-border/60 bg-background/30 p-3">
                <div class="flex items-center justify-between gap-4">
                  <Skeleton class="h-4 w-24" />
                  <Skeleton class="h-4 w-10" />
                </div>
                <Skeleton class="h-2 w-full rounded-full" />
              </div>
            </div>
          {/each}
        </div>
      </CardContent>
    </Card>
  {:else}
    <ImageProcessingCard
      {settings}
      {sources}
      {getSourceAvifEnabled}
      {getSourceAvifQuality}
      {setSourceAvifEnabled}
      {setSourceAvifQuality}
    />
  {/if}
{/snippet}

<Tabs value="all" class="space-y-4">
  <div class="overflow-x-auto border-b border-border/70 pb-2">
    <TabsList class="h-auto w-max flex-wrap justify-start gap-1 bg-transparent p-0 sm:w-fit">
      <TabsTrigger value="all" class={tabTriggerClass}>
        <LayoutGrid class="h-4 w-4" />
        {$t("app.settings.tabs.all")}
      </TabsTrigger>
      <TabsTrigger value="library" class={tabTriggerClass}>
        <BookOpen class="h-4 w-4" />
        {$t("app.settings.tabs.library")}
      </TabsTrigger>
      <TabsTrigger value="storage" class={tabTriggerClass}>
        <Database class="h-4 w-4" />
        {$t("app.settings.tabs.storage")}
      </TabsTrigger>
      <TabsTrigger value="images" class={tabTriggerClass}>
        <Image class="h-4 w-4" />
        {$t("app.settings.tabs.images")}
      </TabsTrigger>
      <TabsTrigger value="integrations" class={tabTriggerClass}>
        <Plug class="h-4 w-4" />
        {$t("app.settings.tabs.integrations")}
      </TabsTrigger>
    </TabsList>
  </div>

  <TabsContent value="all" class="mt-0">
    <div class="grid gap-3 xl:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)] xl:items-start">
      <div class="space-y-3">
        <LibraryUpdatesCard
          bind:settings
          {autoDownload}
          {libraryCategories}
          {triggeringUpdate}
          {updateResult}
          onTriggerUpdate={onTriggerUpdate}
        />

        <DiscordCard bind:settings />

        <DownloadConcurrencyCard bind:settings />

        <AuthCard bind:settings />

        <BackendApiCard bind:settings />

        <DiscoveryCard bind:settings />
      </div>

      <div class="space-y-3">
        <CacheCard
          bind:settings
          {clearingCache}
          {clearCacheResult}
          onClearCache={onClearCache}
          {cleaningDatabase}
          {cleanupDatabaseResult}
          onCleanupDatabase={onCleanupDatabase}
        />

        <ArchiveIndexCard
          status={archiveIndexStatus}
          loading={archiveIndexLoading}
          rebuilding={rebuildingArchiveIndex}
          rebuildResult={archiveIndexRebuildResult}
          onRebuild={onRebuildArchiveIndex}
          cleaningStale={cleaningArchiveIndexStale}
          cleanupResult={archiveIndexCleanupResult}
          onCleanupStale={onCleanupArchiveIndexStale}
          clearingIndex={clearingArchiveIndex}
          clearResult={archiveIndexClearResult}
          onClearIndex={onClearArchiveIndex}
        />

        <AvifSettingsCard
          {getAvifConversionWorkers}
          {setAvifConversionWorkers}
          {reencoding}
          {reencodeResult}
          onReencode={onReencode}
        />

        {@render imageProcessingSettings()}

        <StorageCard bind:settings />
      </div>
    </div>
  </TabsContent>

  <TabsContent value="library" class="mt-0">
    <div class="grid gap-3 lg:grid-cols-2 lg:items-start">
      <LibraryUpdatesCard
        bind:settings
        {autoDownload}
        {libraryCategories}
        {triggeringUpdate}
        {updateResult}
        onTriggerUpdate={onTriggerUpdate}
      />

      <DiscoveryCard bind:settings />

      <DownloadConcurrencyCard bind:settings />
    </div>
  </TabsContent>

  <TabsContent value="storage" class="mt-0">
    <div class="grid gap-3 lg:grid-cols-2 lg:items-start">
      <CacheCard
        bind:settings
        {clearingCache}
        {clearCacheResult}
        onClearCache={onClearCache}
        {cleaningDatabase}
        {cleanupDatabaseResult}
        onCleanupDatabase={onCleanupDatabase}
      />

      <ArchiveIndexCard
        status={archiveIndexStatus}
        loading={archiveIndexLoading}
        rebuilding={rebuildingArchiveIndex}
        rebuildResult={archiveIndexRebuildResult}
        onRebuild={onRebuildArchiveIndex}
        cleaningStale={cleaningArchiveIndexStale}
        cleanupResult={archiveIndexCleanupResult}
        onCleanupStale={onCleanupArchiveIndexStale}
        clearingIndex={clearingArchiveIndex}
        clearResult={archiveIndexClearResult}
        onClearIndex={onClearArchiveIndex}
      />

      <StorageCard bind:settings />
    </div>
  </TabsContent>

  <TabsContent value="images" class="mt-0">
    <div class="grid gap-3 xl:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)] xl:items-start">
      <AvifSettingsCard
        {getAvifConversionWorkers}
        {setAvifConversionWorkers}
        {reencoding}
        {reencodeResult}
        onReencode={onReencode}
      />

      {@render imageProcessingSettings()}
    </div>
  </TabsContent>

  <TabsContent value="integrations" class="mt-0">
    <div class="grid gap-3 lg:grid-cols-2 lg:items-start">
      <DiscordCard bind:settings />

      <div class="space-y-3">
        <AuthCard bind:settings />
        <BackendApiCard bind:settings />
      </div>
    </div>
  </TabsContent>
</Tabs>
