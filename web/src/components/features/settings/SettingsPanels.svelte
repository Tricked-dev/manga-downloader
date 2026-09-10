<script lang="ts">
  import { BookOpen, Database, Image, LayoutGrid, Plug } from "@lucide/svelte";

  import UpscaleSettingsCard from "./UpscaleSettingsCard.svelte";
  import AuthCard from "$components/features/settings/AuthCard.svelte";
  import BackendApiCard from "$components/features/settings/BackendApiCard.svelte";
  import CacheCard from "$components/features/settings/CacheCard.svelte";
  import DiscoveryCard from "$components/features/settings/DiscoveryCard.svelte";
  import DownloadConcurrencyCard from "$components/features/settings/DownloadConcurrencyCard.svelte";
  import LibraryUpdatesCard from "$components/features/settings/LibraryUpdatesCard.svelte";
  import StorageCard from "$components/features/settings/StorageCard.svelte";
  import { Tabs, TabsContent, TabsList, TabsTrigger } from "$lib/ui/tabs";
  import { t } from "$lib/i18n";
  import type { SettingsMap } from "$lib/features/settings/types";
  import type { Source } from "$lib/types";

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
    sources,
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
    sources: Source[];
  }>();

  const tabTriggerClass =
    "h-8 flex-none px-3 text-muted-foreground hover:bg-muted/40 hover:text-foreground focus-visible:border-border/60 focus-visible:ring-1 focus-visible:ring-border/50 focus-visible:outline-none";
</script>



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

        <UpscaleSettingsCard bind:settings {sources} />

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

      <StorageCard bind:settings />
    </div>
  </TabsContent>

  <TabsContent value="images" class="mt-0">
    <div class="grid gap-3 xl:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)] xl:items-start">

      <UpscaleSettingsCard bind:settings {sources} />
    </div>
  </TabsContent>

  <TabsContent value="integrations" class="mt-0">
    <div class="grid gap-3 lg:grid-cols-2 lg:items-start">

      <div class="space-y-3">
        <AuthCard bind:settings />
        <BackendApiCard bind:settings />
      </div>
    </div>
  </TabsContent>
</Tabs>
