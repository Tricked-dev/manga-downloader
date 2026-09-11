<script lang="ts">
  import { Check, Copy, Eye, EyeOff } from "@lucide/svelte";
  import { Button } from "$lib/ui/button";
  import { Input } from "$lib/ui/input";
  import SettingField from "./SettingField.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import SettingsNotice from "./SettingsNotice.svelte";
  import { t } from "$lib/i18n";

  import type { SettingsMap } from "$lib/features/settings/types";

  let { settings = $bindable() } = $props<{ settings: SettingsMap }>();

  const apiKey = $derived(settings.backend_api_key?.trim() ?? "");

  let revealed = $state(false);
  let copied = $state(false);
  let copyFailed = $state(false);

  async function copyApiKey() {
    try {
      await navigator.clipboard.writeText(apiKey);
      copyFailed = false;
      copied = true;
      window.setTimeout(() => {
        copied = false;
      }, 1800);
    } catch {
      copyFailed = true;
    }
  }
</script>

<SettingsCard title={$t("app.settings.backendApi")} description={$t("app.settings.backendApiDescription")}>
  <SettingField
    id="backend-api-key"
    label={$t("app.settings.apiKey")}
    hint={$t("app.settings.apiKeyHint")}
  >
    <div class="flex items-center gap-2">
      <Input
        id="backend-api-key"
        type={revealed ? "text" : "password"}
        class="h-9 font-mono"
        value={apiKey}
        readonly
        autocomplete="off"
        spellcheck={false}
      />
      <Button
        variant="outline"
        size="sm"
        type="button"
        disabled={!apiKey}
        aria-label={revealed ? $t("app.settings.apiKeyHide") : $t("app.settings.apiKeyReveal")}
        onclick={() => (revealed = !revealed)}
      >
        {#if revealed}
          <EyeOff class="size-3.5" />
        {:else}
          <Eye class="size-3.5" />
        {/if}
      </Button>
      <Button variant="outline" size="sm" type="button" disabled={!apiKey} onclick={copyApiKey}>
        {#if copied}
          <Check class="size-3.5" />
          {$t("app.settings.apiKeyCopied")}
        {:else}
          <Copy class="size-3.5" />
          {$t("app.settings.apiKeyCopy")}
        {/if}
      </Button>
    </div>
  </SettingField>

  {#if copyFailed}
    <SettingsNotice>{$t("app.settings.apiKeyCopyFailed")}</SettingsNotice>
  {/if}
</SettingsCard>
