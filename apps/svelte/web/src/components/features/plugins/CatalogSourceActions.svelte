<script lang="ts">
  import { resolve } from "$app/paths";
  import { Button } from "$lib/ui/button";
  import { t } from "$lib/i18n";
  import SourceInfoPopover from "$components/features/plugins/SourceInfoPopover.svelte";
  import type { SourceInfoOpenState } from "$components/features/plugins/source-info-open-state.svelte";
  import type { Source, SourceSettings } from "$lib/types";

  const {
    source,
    sourceSettings,
    loadingSettings,
    savingSettings,
    sourceInfoOpen,
    onHideNsfwChange,
  } = $props<{
    source: Source;
    sourceSettings: Record<string, SourceSettings>;
    loadingSettings: Record<string, boolean>;
    savingSettings: Record<string, boolean>;
    sourceInfoOpen: SourceInfoOpenState;
    onHideNsfwChange: (source: Source, checked: boolean) => void;
  }>();

  function handleOpenChange(open: boolean): void {
    sourceInfoOpen.setOpen(source, open);
  }

  function handleHideNsfwChange(checked: boolean): void {
    onHideNsfwChange(source, checked);
  }
</script>

<Button variant="outline" size="sm" href={resolve(`/sources/${source.name}/search` as `/sources/${string}/search`)}>
  {$t("app.sources.browse")}
</Button>
<SourceInfoPopover
  {source}
  open={sourceInfoOpen.isOpen(source.name)}
  setting={sourceSettings[source.name]}
  loadingSettings={Boolean(loadingSettings[source.name])}
  savingSettings={Boolean(savingSettings[source.name])}
  onOpenChange={handleOpenChange}
  onHideNsfwChange={handleHideNsfwChange}
/>
