<script lang="ts">
  import { t } from "$lib/i18n";
  import SettingsCard from "./SettingsCard.svelte";
  import SourceImageProcessingItem from "./SourceImageProcessingItem.svelte";

  import type { SettingsMap, SettingsSource } from "$lib/features/settings/types";

  const {
    settings,
    sources,
    getSourceAvifEnabled,
    getSourceAvifQuality,
    setSourceAvifEnabled,
    setSourceAvifQuality,
  }: {
    settings: SettingsMap;
    sources: SettingsSource[];
    getSourceAvifEnabled: (sourceName: string) => boolean;
    getSourceAvifQuality: (sourceName: string) => number;
    setSourceAvifEnabled: (sourceName: string, enabled: boolean) => void;
    setSourceAvifQuality: (sourceName: string, quality: number) => void;
  } = $props();
</script>

<SettingsCard
  title={$t("app.settings.imageProcessing.title")}
  description={$t("app.settings.imageProcessing.description")}
  contentClass=""
>
  {#if sources.length > 0}
    <div class="space-y-3">
      {#each sources as source (source.name)}
        <SourceImageProcessingItem
          {source}
          {getSourceAvifEnabled}
          {getSourceAvifQuality}
          {setSourceAvifEnabled}
          {setSourceAvifQuality}
        />
      {/each}
    </div>
  {/if}
</SettingsCard>
