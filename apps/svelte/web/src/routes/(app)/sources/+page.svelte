<script lang="ts">
  import { browser } from "$app/environment";
  import {
    createDeleteSource,
    createListSources,
    createSetSourceEnabled,
    createUpdateSourceSettings,
    createUploadPlugin,
    getGetSourceSettingsQueryKey,
    getGetSourceSettingsQueryOptions,
    getListSourceArtifactsQueryKey,
    getListSourceArtifactsQueryOptions,
    getListSourcesQueryKey,
  } from "@manga-server/api-client/generated";
  import { useHydrate, useQueryClient, type DehydratedState } from "@tanstack/svelte-query";
  import type SourcesPanelsComponent from "$components/features/plugins/SourcesPanels.svelte";
  import { untrack } from "svelte";
  import { Alert, AlertDescription, AlertTitle } from "$lib/ui/alert";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { Skeleton } from "$lib/ui/skeleton";
  import { Tabs, TabsContent, TabsList, TabsTrigger } from "$lib/ui/tabs";
  import PageHeader from "@manga-server/ui/components/page-header";
  import EmptyState from "$components/common/EmptyState.svelte";
  import { t } from "$lib/i18n";
  import { selectItems } from "$lib/query-selectors";
  import type { PluginArtifact, Source, SourceSettings } from "$lib/types";
  import { QUERY_CACHE_TIMES } from "$lib/query-client";
  import { dehydratedQueryOptions } from "$lib/query-hydration";
  import { Boxes, Search, Upload } from "@lucide/svelte";

  const EMPTY_SOURCES: Source[] = [];

  const { data } = $props<{
    data: {
      dehydratedState?: DehydratedState;
    };
  }>();

  let SourcesPanels = $state<typeof SourcesPanelsComponent | null>(null);
  const queryClient = useQueryClient();

  useHydrate(untrack(() => data.dehydratedState), undefined, queryClient);

  if (browser) {
    void import("$components/features/plugins/SourcesPanels.svelte").then((module) => {
      SourcesPanels = module.default;
    });
  }

  const sourcesQuery = createListSources(() => ({
    query: {
      ...dehydratedQueryOptions(data.dehydratedState, getListSourcesQueryKey()),
      select: selectItems<Source>,
      staleTime: QUERY_CACHE_TIMES.stable,
    },
  }));

  const sources: Source[] = $derived(sourcesQuery.data ?? EMPTY_SOURCES);
  const activeSources = $derived(getActiveSources());
  const loading = $derived(sourcesQuery.isPending);
  let error = $state("");
  let uploadKey = $state(Date.now()); // Re-render input slightly on clear
  let uploading = $state(false);
  const toggling = $state<Record<string, boolean>>({});
  const sourceSettings = $state<Record<string, SourceSettings>>({});
  const sourceArtifacts = $state<Record<string, PluginArtifact[]>>({});
  const loadingSettings = $state<Record<string, boolean>>({});
  const loadingArtifacts = $state<Record<string, boolean>>({});
  const savingSettings = $state<Record<string, boolean>>({});
  const deletingSources = $state<Record<string, boolean>>({});
  let activeView = $state<"catalog" | "plugins">("catalog");

  const toggleSourceMutation = createSetSourceEnabled();

  const uploadSourceMutation = createUploadPlugin();

  const updateSettingsMutation = createUpdateSourceSettings();

  const deleteSourceMutation = createDeleteSource();

  function getActiveSources() {
    return sources.filter((source: Source) => source.enabled);
  }

  async function refreshSources() {
    await queryClient.invalidateQueries({ queryKey: getListSourcesQueryKey() });
  }

  async function handleFileUpload(e: Event) {
    const target = e.target as HTMLInputElement;
    if (!target.files || target.files.length === 0) {return;}

    uploading = true;

    const file = target.files[0];
    try {
      await runWithError(async () => {
        await uploadSourceMutation.mutateAsync({ data: { file } });
        await refreshSources();
        uploadKey = Date.now();
      });
    } finally {
      uploading = false;
    }
  }

  async function setSourceEnabled(source: Source, enabled: boolean) {
    toggling[source.name] = true;

    try {
      await runWithError(async () => {
        await toggleSourceMutation.mutateAsync({ data: { enabled }, name: source.name });
        await refreshSources();
      });
    } finally {
      delete toggling[source.name];
    }
  }

  async function ensureSourceSettings(source: Source) {
    if (sourceSettings[source.name] || loadingSettings[source.name]) {return;}

    loadingSettings[source.name] = true;
    try {
      await runWithError(async () => {
        sourceSettings[source.name] = await queryClient.fetchQuery({
          ...getGetSourceSettingsQueryOptions(source.name),
          staleTime: QUERY_CACHE_TIMES.stable,
        });
      });
    } finally {
      delete loadingSettings[source.name];
    }
  }

  async function ensureSourceArtifacts(source: Source) {
    if (sourceArtifacts[source.name] || loadingArtifacts[source.name]) {return;}

    loadingArtifacts[source.name] = true;
    try {
      await runWithError(async () => {
        const response = await queryClient.fetchQuery({
          ...getListSourceArtifactsQueryOptions(source.name),
          staleTime: QUERY_CACHE_TIMES.stable,
        });
        sourceArtifacts[source.name] = response.items;
      });
    } finally {
      delete loadingArtifacts[source.name];
    }
  }

  async function setHideNsfw(source: Source, hideNsfw: boolean) {
    savingSettings[source.name] = true;

    try {
      await runWithError(async () => {
        const nextSettings = await updateSettingsMutation.mutateAsync({
          data: {
            hide_nsfw: hideNsfw,
          },
          name: source.name,
        });
        queryClient.setQueryData(
          getGetSourceSettingsQueryKey(source.name),
          nextSettings,
        );
        sourceSettings[source.name] = nextSettings;
      });
    } finally {
      delete savingSettings[source.name];
    }
  }

  async function runWithError(task: () => Promise<void>) {
    error = "";

    try {
      await task();
    } catch (caught) {
      error = caught instanceof Error ? caught.message : $t("app.errors.generic");
    }
  }

  async function loadSourceDetails(source: Source) {
    await Promise.all([ensureSourceSettings(source), ensureSourceArtifacts(source)]);
  }

  async function deleteSource(source: Source) {
    if (!confirm($t("app.sources.deleteConfirm", { source: source.display_name }))) {
      return;
    }

    deletingSources[source.name] = true;
    try {
      await runWithError(async () => {
        await deleteSourceMutation.mutateAsync({ name: source.name });
        delete sourceSettings[source.name];
        delete sourceArtifacts[source.name];
        queryClient.removeQueries({ queryKey: getGetSourceSettingsQueryKey(source.name) });
        queryClient.removeQueries({ queryKey: getListSourceArtifactsQueryKey(source.name) });
        await refreshSources();
      });
    } finally {
      delete deletingSources[source.name];
    }
  }

  function setActiveView(value: string) {
    activeView = value === "plugins" ? "plugins" : "catalog";
  }
</script>

<svelte:head>
  <title>{$t("app.sources.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<Tabs
  value={activeView}
  onValueChange={setActiveView}
  class="space-y-6"
>
  <PageHeader title={$t("app.sources.title")} description={$t("app.sources.description")}>
    {#snippet actions()}
      <TabsList class="grid w-full grid-cols-2 sm:w-auto">
        <TabsTrigger value="catalog">
          <Search class="h-4 w-4" />
          {$t("app.sources.catalog")}
        </TabsTrigger>
        <TabsTrigger value="plugins">
          <Boxes class="h-4 w-4" />
          {$t("app.sources.plugins")}
        </TabsTrigger>
      </TabsList>
    {/snippet}
  </PageHeader>

  {#if error}
    <Alert variant="destructive">
      <AlertTitle>{$t("app.sources.requestFailed")}</AlertTitle>
      <AlertDescription>{error}</AlertDescription>
    </Alert>
  {/if}

  <Card class="gap-0 overflow-visible">
    <CardHeader class="border-b border-border/70 pb-4">
      <CardTitle>{activeView === "catalog" ? $t("app.sources.catalog") : $t("app.sources.installedPlugins")}</CardTitle>
      {#if activeView === "catalog"}
        <CardDescription>
          {$t("app.sources.catalogCount", { count: activeSources.length, suffix: activeSources.length === 1 ? "" : "s" })}
        </CardDescription>
      {:else}
        <CardDescription>
          {$t("app.sources.installedCount", { installed: sources.length, enabled: activeSources.length })}
        </CardDescription>
      {/if}
    </CardHeader>
    <CardContent class="overflow-visible p-0">
      {#if loading || !SourcesPanels}
        <div class="divide-y divide-border">
          {#each Array.from({ length: 3 }, (_, index) => index) as index (index)}
            <div class="flex items-center justify-between px-4 py-4">
              <Skeleton class="h-6 w-32" />
              <Skeleton class="h-6 w-16" />
            </div>
          {/each}
        </div>
      {:else if sources.length === 0}
        <EmptyState
          icon={Upload}
          title={$t("app.sources.noSources")}
          description={$t("app.sources.noSourcesDescription")}
        />
      {:else}
        <SourcesPanels
          {activeSources}
          {deletingSources}
          {handleFileUpload}
          {loadSourceDetails}
          {loadingArtifacts}
          {loadingSettings}
          {savingSettings}
          setHideNsfw={setHideNsfw}
          setSourceEnabled={setSourceEnabled}
          {sourceArtifacts}
          {sourceSettings}
          {sources}
          {toggling}
          {uploadKey}
          {uploading}
          {deleteSource}
          {ensureSourceSettings}
        />
      {/if}
    </CardContent>
  </Card>
</Tabs>
