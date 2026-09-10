<script lang="ts">
  import { RefreshCw } from "@lucide/svelte";

  import { Button } from "$lib/ui/button";
  import { Select, SelectContent, SelectItem, SelectTrigger } from "$lib/ui/select";
  import { Switch } from "$lib/ui/switch";
  import { t } from "$lib/i18n";
  import { UPDATE_INTERVAL_OPTIONS } from "$lib/settings";
  import SettingField from "./SettingField.svelte";
  import SettingsCard from "./SettingsCard.svelte";

  import type { SettingsMap } from "$lib/features/settings/types";

  const ALL_CATEGORIES_VALUE = "__all_categories__";

  let {
    settings = $bindable(),
    autoDownload,
    libraryCategories,
    triggeringUpdate,
    updateResult,
    onTriggerUpdate,
  }: {
    settings: SettingsMap;
    autoDownload: boolean;
    libraryCategories: string[];
    triggeringUpdate: boolean;
    updateResult: string;
    onTriggerUpdate: () => void;
  } = $props<{
    settings: SettingsMap;
    autoDownload: boolean;
    libraryCategories: string[];
    triggeringUpdate: boolean;
    updateResult: string;
    onTriggerUpdate: () => void;
  }>();

  const selectedIntervalOption = $derived(
    UPDATE_INTERVAL_OPTIONS.find((option) => option.value === settings.update_interval_hours),
  );

  function setAutoDownload(checked: boolean) {
    settings.auto_download_new_chapters = checked ? "true" : "false";
  }

  function updateAutoDownloadCategory(value: string) {
    settings.auto_download_category = value === ALL_CATEGORIES_VALUE ? "" : value;
  }
</script>

<SettingsCard title={$t("app.settings.libraryUpdates")} description={$t("app.settings.libraryUpdatesDescription")}>
  <SettingField id="interval" label={$t("app.settings.interval")}>
    <Select type="single" bind:value={settings.update_interval_hours}>
      <SelectTrigger class="h-9 w-full" id="interval">
        {#if selectedIntervalOption}
          {$t(selectedIntervalOption.labelKey)}
        {:else}
          {$t("app.settings.selectInterval")}
        {/if}
      </SelectTrigger>
      <SelectContent>
        {#each UPDATE_INTERVAL_OPTIONS as opt (opt.value)}
          <SelectItem value={opt.value}>{$t(opt.labelKey)}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
  </SettingField>

  <div class="flex items-start justify-between gap-4">
    <SettingField
      id="auto-dl"
      label={$t("app.settings.autoDownloadNewChapters")}
      hint={$t("app.settings.autoDownloadNewChaptersHint")}
      class="space-y-0.5"
    />
    <Switch
      id="auto-dl"
      checked={autoDownload}
      onCheckedChange={setAutoDownload}
    />
  </div>

  <SettingField
    id="auto-download-category"
    label={$t("app.settings.autoDownloadCategory")}
    hint={$t("app.settings.autoDownloadCategoryHint")}
  >
    <Select
      type="single"
      value={settings.auto_download_category || ALL_CATEGORIES_VALUE}
      onValueChange={updateAutoDownloadCategory}
      disabled={!autoDownload}
    >
      <SelectTrigger class="h-9 w-full" id="auto-download-category">
        {settings.auto_download_category || $t("app.library.allCategories")}
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={ALL_CATEGORIES_VALUE}>{$t("app.library.allCategories")}</SelectItem>
        {#each libraryCategories as category (category)}
          <SelectItem value={category}>{category}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
  </SettingField>

  <div class="pt-0.5">
    <Button
      variant="secondary"
      onclick={onTriggerUpdate}
      disabled={triggeringUpdate}
      class="h-9"
    >
      {#if triggeringUpdate}
        {$t("app.updates.checking")}
      {:else}
        <RefreshCw class="mr-2 h-4 w-4" /> {$t("app.updates.checkNow")}
      {/if}
    </Button>
    {#if updateResult}
      <p class="mt-2 text-sm text-muted-foreground">{updateResult}</p>
    {/if}
  </div>
</SettingsCard>
