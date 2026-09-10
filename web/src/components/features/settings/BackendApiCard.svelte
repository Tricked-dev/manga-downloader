<script lang="ts">
  import { Input } from "$lib/ui/input";
  import SettingField from "./SettingField.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import SettingsNotice from "./SettingsNotice.svelte";
  import { t } from "$lib/i18n";

  import type { SettingsMap } from "$lib/features/settings/types";

  let { settings = $bindable() } = $props<{ settings: SettingsMap }>();

  const apiKeyConfigured = $derived(Boolean(settings.backend_api_key?.trim()));
</script>

<SettingsCard title={$t("app.settings.backendApi")} description={$t("app.settings.backendApiDescription")}>
  <SettingField
    id="backend-api-key"
    label={$t("app.settings.apiKey")}
    hint={$t("app.settings.apiKeyHint")}
  >
    <Input
      id="backend-api-key"
      type="password"
      class="h-9"
      bind:value={settings.backend_api_key}
      autocomplete="new-password"
      placeholder={$t("app.settings.apiKeyPlaceholder")}
    />
  </SettingField>

  {#if apiKeyConfigured}
    <SettingsNotice>{$t("app.settings.apiRestartNotice")}</SettingsNotice>
  {/if}
</SettingsCard>
