<script lang="ts">
  import { createListSources, createSetSourceEnabled, createUpdateSourceSettings, getGetSourceSettingsQueryKey, getGetSourceSettingsQueryOptions, getListSourcesQueryKey } from "@manga-server/api-client/generated";
  import { useQueryClient } from "@tanstack/svelte-query";
  import { Alert, AlertDescription, AlertTitle } from "$lib/ui/alert";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { Switch } from "$lib/ui/switch";
  import { Tabs, TabsContent, TabsList, TabsTrigger } from "$lib/ui/tabs";
  import PageHeader from "@manga-server/ui/components/page-header";
  import SourcesCatalogTable from "$components/features/plugins/SourcesCatalogTable.svelte";
  import SourceSummaryCell from "$components/features/plugins/SourceSummaryCell.svelte";
  import { t } from "$lib/i18n";
  import { selectItems } from "$lib/query-selectors";
  import type { Source, SourceSettings } from "$lib/types";
  import { QUERY_CACHE_TIMES } from "$lib/query-client";

  const queryClient = useQueryClient();
  const sourcesQuery = createListSources(() => ({ query: { select: selectItems<Source>, staleTime: QUERY_CACHE_TIMES.stable } }));
  const sources = $derived(sourcesQuery.data ?? []);
  const activeSources = $derived(sources.filter((source: Source) => source.enabled));
  let error = $state("");
  const toggling = $state<Record<string, boolean>>({});
  const sourceSettings = $state<Record<string, SourceSettings>>({});
  const loadingSettings = $state<Record<string, boolean>>({});
  const savingSettings = $state<Record<string, boolean>>({});
  const toggleSourceMutation = createSetSourceEnabled();
  const updateSettingsMutation = createUpdateSourceSettings();

  async function setSourceEnabled(source: Source, enabled: boolean) {
    toggling[source.name] = true;
    error = "";
    try {
      await toggleSourceMutation.mutateAsync({ data: { enabled }, name: source.name });
      await queryClient.invalidateQueries({ queryKey: getListSourcesQueryKey() });
    } catch (caught) { error = caught instanceof Error ? caught.message : $t("app.errors.generic"); }
    finally { delete toggling[source.name]; }
  }
  async function ensureSourceSettings(source: Source) {
    if (sourceSettings[source.name] || loadingSettings[source.name]) return;
    loadingSettings[source.name] = true;
    try {
      sourceSettings[source.name] = await queryClient.fetchQuery({ ...getGetSourceSettingsQueryOptions(source.name), staleTime: QUERY_CACHE_TIMES.stable });
    } catch (caught) { error = caught instanceof Error ? caught.message : $t("app.errors.generic"); }
    finally { delete loadingSettings[source.name]; }
  }
  async function setHideNsfw(source: Source, hideNsfw: boolean) {
    savingSettings[source.name] = true;
    try {
      const next = await updateSettingsMutation.mutateAsync({ data: { hide_nsfw: hideNsfw }, name: source.name });
      queryClient.setQueryData(getGetSourceSettingsQueryKey(source.name), next);
      sourceSettings[source.name] = next;
    } catch (caught) { error = caught instanceof Error ? caught.message : $t("app.errors.generic"); }
    finally { delete savingSettings[source.name]; }
  }
</script>

<svelte:head><title>{$t("app.sources.title")} | {$t("app.appLogo.title")}</title></svelte:head>
<PageHeader title={$t("app.sources.title")} description={$t("app.sources.description")} />
{#if error}
  <Alert variant="destructive"><AlertTitle>{$t("app.sources.requestFailed")}</AlertTitle><AlertDescription>{error}</AlertDescription></Alert>
{/if}
<Tabs value="catalog" class="space-y-4">
  <TabsList><TabsTrigger value="catalog">{$t("app.sources.catalog")}</TabsTrigger><TabsTrigger value="manage">{$t("app.sources.manage")}</TabsTrigger></TabsList>
  <TabsContent value="catalog">
    <Card class="overflow-visible gap-0">
      <CardHeader><CardTitle>{$t("app.sources.enabledSources")}</CardTitle><CardDescription>{$t("app.sources.catalogCount", { count: activeSources.length, suffix: activeSources.length === 1 ? "" : "s" })}</CardDescription></CardHeader>
      <CardContent class="p-0">
        {#if sourcesQuery.isPending}<p class="p-4 text-muted-foreground">{$t("app.sources.loading")}</p>
        {:else if activeSources.length === 0}<p class="p-4 text-muted-foreground">{$t("app.sources.noEnabledSources")}</p>
        {:else}<SourcesCatalogTable sources={activeSources} {sourceSettings} {loadingSettings} {savingSettings} onOpenInfo={ensureSourceSettings} onHideNsfwChange={setHideNsfw} />{/if}
      </CardContent>
    </Card>
  </TabsContent>
  <TabsContent value="manage">
    <Card><CardHeader><CardTitle>{$t("app.sources.available")}</CardTitle><CardDescription>{$t("app.sources.manageDescription")}</CardDescription></CardHeader>
      <CardContent class="divide-y divide-border">
        {#each sources as source (source.name)}
          <div class="flex items-center justify-between gap-4 py-4"><SourceSummaryCell {source} /><Switch aria-label={$t("app.sources.enable", { source: source.display_name })} checked={source.enabled} disabled={Boolean(toggling[source.name])} onCheckedChange={(enabled) => setSourceEnabled(source, enabled)} /></div>
        {/each}
      </CardContent>
    </Card>
  </TabsContent>
</Tabs>
