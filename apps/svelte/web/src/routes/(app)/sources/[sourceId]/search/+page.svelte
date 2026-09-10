<script lang="ts">
  import SourceSearchView from "$components/features/plugins/SourceSearchView.svelte";
  import { t } from "$lib/i18n";
  import type { Source } from "$lib/types";

  const { data } = $props<{ data: { sourceInfo: Source } }>();
  const sourceInfo = $derived(data.sourceInfo);
  const sourceName = $derived(sourceInfo.name);
</script>

<svelte:head>
  <title>{$t("app.sources.search.pageTitle", { source: sourceName })} | {$t("app.appLogo.title")}</title>
</svelte:head>

{#key sourceName}
  <SourceSearchView {sourceName} {sourceInfo} />
{/key}
