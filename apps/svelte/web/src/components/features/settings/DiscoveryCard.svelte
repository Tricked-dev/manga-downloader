<script lang="ts">
  import { Input } from "$lib/ui/input";
  import SettingField from "./SettingField.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import { t } from "$lib/i18n";

  import type { SettingsMap } from "$lib/features/settings/types";

  let { settings = $bindable() } = $props<{ settings: SettingsMap }>();
</script>

<SettingsCard title={$t("app.settings.discovery.title")} description={$t("app.settings.discovery.description")}>
  <SettingField
    id="library-categories"
    label={$t("app.settings.discovery.categories")}
    hint={$t("app.settings.discovery.categoriesHint")}
  >
    <Input
      id="library-categories"
      type="text"
      class="h-9"
      bind:value={settings.library_categories}
      placeholder="default,downloaded"
    />
  </SettingField>
</SettingsCard>
