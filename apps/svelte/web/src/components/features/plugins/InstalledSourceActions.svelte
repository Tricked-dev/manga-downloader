<script lang="ts">
  import { Switch } from "$lib/ui/switch";
  import SourceInfoPopover from "$components/features/plugins/SourceInfoPopover.svelte";
  import type { SourceInfoOpenState } from "$components/features/plugins/source-info-open-state.svelte";
  import type { PluginArtifact, Source, SourceSettings } from "$lib/types";

  const EMPTY_ARTIFACTS: PluginArtifact[] = [];

  const {
    source,
    toggling,
    sourceSettings,
    sourceArtifacts,
    loadingSettings,
    loadingArtifacts,
    savingSettings,
    deletingSources,
    sourceInfoOpen,
    onHideNsfwChange,
    onDelete,
    onToggle,
    showToggle = true,
    showInfo = true,
  } = $props<{
    source: Source;
    toggling: Record<string, boolean>;
    sourceSettings: Record<string, SourceSettings>;
    sourceArtifacts: Record<string, PluginArtifact[]>;
    loadingSettings: Record<string, boolean>;
    loadingArtifacts: Record<string, boolean>;
    savingSettings: Record<string, boolean>;
    deletingSources: Record<string, boolean>;
    sourceInfoOpen: SourceInfoOpenState;
    onHideNsfwChange: (source: Source, checked: boolean) => void;
    onDelete: (source: Source) => void;
    onToggle: (source: Source, enabled: boolean) => void;
    showToggle?: boolean;
    showInfo?: boolean;
  }>();

  const artifacts = $derived(sourceArtifacts[source.name] ?? EMPTY_ARTIFACTS);

  function handleToggle(checked: boolean): void {
    onToggle(source, checked);
  }

  function handleOpenChange(open: boolean): void {
    sourceInfoOpen.setOpen(source, open);
  }

  function handleHideNsfwChange(checked: boolean): void {
    onHideNsfwChange(source, checked);
  }

  function handleDelete(): void {
    onDelete(source);
  }
</script>

{#if showToggle}
  <Switch
    checked={source.enabled}
    disabled={Boolean(toggling[source.name])}
    onCheckedChange={handleToggle}
  />
{/if}
{#if showInfo}
  <SourceInfoPopover
    {source}
    open={sourceInfoOpen.isOpen(source.name)}
    setting={sourceSettings[source.name]}
    loadingSettings={Boolean(loadingSettings[source.name])}
    savingSettings={Boolean(savingSettings[source.name])}
    {artifacts}
    loadingArtifacts={Boolean(loadingArtifacts[source.name])}
    deleting={Boolean(deletingSources[source.name])}
    onOpenChange={handleOpenChange}
    onHideNsfwChange={handleHideNsfwChange}
    onDelete={handleDelete}
  />
{/if}
