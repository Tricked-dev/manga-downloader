<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { Input } from "$lib/ui/input";
  import { Tabs, TabsContent, TabsList, TabsTrigger } from "$lib/ui/tabs";
  import EmptyState from "$components/common/EmptyState.svelte";
  import SourcesCatalogTable from "$components/features/plugins/SourcesCatalogTable.svelte";
  import InstalledSourcesTable from "$components/features/plugins/InstalledSourcesTable.svelte";
  import { t } from "$lib/i18n";
  import { Upload } from "@lucide/svelte";
  import type { PluginArtifact, Source, SourceSettings } from "$lib/types";

  const {
    activeSources,
    deletingSources,
    handleFileUpload,
    loadSourceDetails,
    loadingArtifacts,
    loadingSettings,
    savingSettings,
    setHideNsfw,
    setSourceEnabled,
    sourceArtifacts,
    sourceSettings,
    sources,
    toggling,
    uploadKey,
    uploading,
    deleteSource,
    ensureSourceSettings,
  } = $props<{
    activeSources: Source[];
    deletingSources: Record<string, boolean>;
    handleFileUpload: (event: Event) => void;
    loadSourceDetails: (source: Source) => void | Promise<void>;
    loadingArtifacts: Record<string, boolean>;
    loadingSettings: Record<string, boolean>;
    savingSettings: Record<string, boolean>;
    setHideNsfw: (source: Source, hideNsfw: boolean) => void;
    setSourceEnabled: (source: Source, enabled: boolean) => void;
    sourceArtifacts: Record<string, PluginArtifact[]>;
    sourceSettings: Record<string, SourceSettings>;
    sources: Source[];
    toggling: Record<string, boolean>;
    uploadKey: number;
    uploading: boolean;
    deleteSource: (source: Source) => void;
    ensureSourceSettings: (source: Source) => void | Promise<void>;
  }>();

</script>

<div class="p-4">
  <TabsContent value="catalog">
    <section class="overflow-hidden rounded-md border border-border/70">
      <header class="border-b border-border/70 px-4 py-3">
        <h2 class="text-base font-semibold leading-none tracking-tight">{$t("app.sources.enabledSources")}</h2>
      </header>
      {#if activeSources.length === 0}
        <EmptyState
          icon={Upload}
          title={$t("app.sources.noEnabledSources")}
          description={$t("app.sources.noEnabledSourcesDescription")}
        />
      {:else}
        <SourcesCatalogTable
          sources={activeSources}
          {sourceSettings}
          {loadingSettings}
          {savingSettings}
          onOpenInfo={ensureSourceSettings}
          onHideNsfwChange={setHideNsfw}
        />
      {/if}
    </section>
  </TabsContent>

  <TabsContent value="plugins">
    <section class="overflow-hidden rounded-md border border-border/70">
      <header class="flex flex-col gap-3 border-b border-border/70 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
        <div class="space-y-1">
          <h2 class="text-base font-semibold leading-none tracking-tight">{$t("app.sources.installedPlugins")}</h2>
          <p class="text-sm text-muted-foreground">
            {$t("app.sources.installedPluginsDescription")}
          </p>
        </div>
        <div>
          {#key uploadKey}
            <div class="relative">
              <Input
                type="file"
                accept=".wasm"
                onchange={handleFileUpload}
                disabled={uploading}
                class="absolute inset-0 z-10 h-full w-full cursor-pointer opacity-0"
              />
              <Button disabled={uploading} class="h-9 px-4">
                <Upload class="mr-2 h-4 w-4" />
                {uploading ? $t("app.sources.uploading") : $t("app.sources.upload")}
              </Button>
            </div>
          {/key}
        </div>
      </header>
      <InstalledSourcesTable
        {sources}
        {toggling}
        {sourceSettings}
        {sourceArtifacts}
        {loadingSettings}
        {loadingArtifacts}
        {savingSettings}
        {deletingSources}
        onOpenInfo={loadSourceDetails}
        onHideNsfwChange={setHideNsfw}
        onDelete={deleteSource}
        onToggle={setSourceEnabled}
      />
    </section>
  </TabsContent>
</div>
