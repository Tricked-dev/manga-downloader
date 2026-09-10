<script lang="ts">
  import { Badge } from "$lib/ui/badge";
  import { Label } from "$lib/ui/label";
  import { Slider } from "$lib/ui/slider";
  import { Switch } from "$lib/ui/switch";
  import { t } from "$lib/i18n";
  import SettingField from "./SettingField.svelte";
  import SettingInfoHint from "./SettingInfoHint.svelte";

  import type { SettingsSource } from "$lib/features/settings/types";

  const {
    source,
    getSourceAvifEnabled,
    getSourceAvifQuality,
    setSourceAvifEnabled,
    setSourceAvifQuality,
  }: {
    source: SettingsSource;
    getSourceAvifEnabled: (sourceName: string) => boolean;
    getSourceAvifQuality: (sourceName: string) => number;
    setSourceAvifEnabled: (sourceName: string, enabled: boolean) => void;
    setSourceAvifQuality: (sourceName: string, quality: number) => void;
  } = $props();

  function handleAvifEnabledChange(checked: boolean) {
    setSourceAvifEnabled(source.name, checked);
  }

  function handleAvifQualityChange(value: number) {
    setSourceAvifQuality(source.name, value);
  }
</script>

<div class="space-y-3 rounded-md border border-border/70 bg-card/50 p-3.5">
  <div class="flex items-center justify-between gap-4">
    <div>
      <p class="text-sm font-medium">{source.name}</p>
      <p class="text-xs text-muted-foreground">{source.base_url}</p>
    </div>
    <Badge variant="secondary">{source.enabled ? $t("app.settings.imageProcessing.enabled") : $t("app.settings.imageProcessing.disabled")}</Badge>
  </div>

  <div class="flex items-start justify-between gap-4">
    <SettingField
      id={"source-avif-" + source.name}
      label={$t("app.settings.imageProcessing.convertToAvif")}
      hint={$t("app.settings.imageProcessing.convertToAvifHint")}
      class="space-y-0.5"
    />
    <Switch
      id={"source-avif-" + source.name}
      checked={getSourceAvifEnabled(source.name)}
      onCheckedChange={handleAvifEnabledChange}
    />
  </div>

  <div class="space-y-2.5 rounded-md border border-border/60 bg-background/30 p-3 shadow-[inset_0_1px_0_rgb(255_255_255_/_0.03)]">
    <div class="flex items-center justify-between gap-4">
      <div class="space-y-0.5">
        <div class="flex items-center gap-2">
          <Label for={"source-avif-quality-" + source.name}>{$t("app.settings.imageProcessing.avifQuality")}</Label>
          <SettingInfoHint content={$t("app.settings.imageProcessing.avifQualityHint")} />
        </div>
      </div>
      <span class="min-w-12 text-right text-sm font-medium">{getSourceAvifQuality(source.name)}%</span>
    </div>
    <Slider
      id={"source-avif-quality-" + source.name}
      type="single"
      value={getSourceAvifQuality(source.name)}
      min={10}
      max={100}
      step={1}
      onValueChange={handleAvifQualityChange}
    />
  </div>
</div>
