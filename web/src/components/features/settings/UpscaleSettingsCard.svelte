<script lang="ts">
  import { Switch } from "$lib/ui/switch";
  import { t } from "$lib/i18n";
  import SettingsCard from "./SettingsCard.svelte";
  import type { SettingsMap } from "$lib/features/settings/types";
  import type { Source } from "$lib/types";
  let { settings = $bindable(), sources } = $props<{ settings: SettingsMap; sources: Source[] }>();
</script>

<SettingsCard title={$t("app.upscale.title")} description={$t("app.upscale.description")}>
  <label class="flex items-center justify-between gap-4 rounded-md border border-border/70 p-3.5">
    <span class="text-sm font-medium">{$t("app.upscale.automatic")}</span>
    <Switch aria-label={$t("app.upscale.automatic")} checked={settings.auto_upscale !== "false"} onCheckedChange={(enabled) => settings.auto_upscale = String(enabled)} />
  </label>
  {#each sources as source (source.name)}
    <label class="mt-3 flex items-center justify-between gap-4 rounded-md border border-border/70 p-3.5">
      <span class="text-sm">{source.display_name}</span>
      <Switch aria-label={`${$t("app.upscale.title")}: ${source.display_name}`} checked={settings[`source.${source.name}.auto_upscale`] !== "false"} disabled={settings.auto_upscale === "false"} onCheckedChange={(enabled) => settings[`source.${source.name}.auto_upscale`] = String(enabled)} />
    </label>
  {/each}
  <p class="mt-3 text-xs text-muted-foreground">{$t("app.upscale.backgroundHint")}</p>
</SettingsCard>
