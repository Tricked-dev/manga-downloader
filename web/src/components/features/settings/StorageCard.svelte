<script lang="ts">
  import { Input } from "$lib/ui/input";
  import { Select, SelectContent, SelectItem, SelectTrigger } from "$lib/ui/select";
  import { Switch } from "$lib/ui/switch";
  import {
    encodeBinaryUnitSetting,
    parseBinaryUnitSetting,
  } from "$lib/features/settings/utils";
  import { t } from "$lib/i18n";
  import SettingField from "./SettingField.svelte";
  import SettingsCard from "./SettingsCard.svelte";

  import type { SettingsMap } from "$lib/features/settings/types";

  const STORAGE_LIMIT_DEFAULTS = {
    defaultAmount: "25",
    defaultUnit: "GiB",
    enabledFallback: false,
  } as const;

  let { settings = $bindable() } = $props<{ settings: SettingsMap }>();

  const storageLimit = $derived(parseBinaryUnitSetting(settings.max_download_storage_bytes, STORAGE_LIMIT_DEFAULTS));

  function updateStorageEnabled(checked: boolean) {
    if (!checked) {
      settings.max_download_storage_bytes = "";
      return;
    }

    settings.max_download_storage_bytes = encodeBinaryUnitSetting(storageLimit.amount, storageLimit.unit);
  }

  function updateStorageAmount(value: string) {
    if (!storageLimit.enabled) {
      return;
    }

    settings.max_download_storage_bytes = encodeBinaryUnitSetting(value, storageLimit.unit);
  }

  function updateStorageUnit(value: string) {
    if (!storageLimit.enabled) {
      return;
    }

    settings.max_download_storage_bytes = encodeBinaryUnitSetting(
      storageLimit.amount,
      value === "MiB" ? "MiB" : "GiB",
    );
  }

  function handleStorageAmountInput(event: Event) {
    updateStorageAmount((event.currentTarget as HTMLInputElement).value);
  }
</script>

<SettingsCard title={$t("app.settings.storage.title")} description={$t("app.settings.storage.description")}>
  <SettingField id="dl-path" label={$t("app.settings.storage.downloadPath")}>
    <Input
      id="dl-path"
      type="text"
      class="h-9 font-mono"
      bind:value={settings.download_path}
    />
  </SettingField>

  <div class="flex items-start justify-between gap-4">
    <SettingField
      id="storage-limit-enabled"
      label={$t("app.settings.storage.maxDownloadStorage")}
      hint={$t("app.settings.storage.maxDownloadStorageHint")}
      class="space-y-0.5"
    />
    <Switch
      id="storage-limit-enabled"
      checked={storageLimit.enabled}
      onCheckedChange={updateStorageEnabled}
    />
  </div>

  <SettingField
    id="storage-limit"
    label={$t("app.settings.storage.cap")}
    hint={$t("app.settings.storage.capHint")}
  >
    <div class="grid gap-2 sm:grid-cols-[minmax(0,1fr)_8rem]">
      <Input
        id="storage-limit"
        type="number"
        min="1"
        step="1"
        class="h-9"
        value={storageLimit.amount}
        oninput={handleStorageAmountInput}
        disabled={!storageLimit.enabled}
      />
      <Select
        type="single"
        value={storageLimit.unit}
        onValueChange={updateStorageUnit}
        disabled={!storageLimit.enabled}
      >
        <SelectTrigger class="h-9 w-full">
          {storageLimit.unit}
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="MiB">MiB</SelectItem>
          <SelectItem value="GiB">GiB</SelectItem>
        </SelectContent>
      </Select>
    </div>
  </SettingField>
</SettingsCard>
