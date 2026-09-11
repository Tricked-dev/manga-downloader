<script lang="ts">
  import { Check, Copy, Eye, EyeOff, Loader2, RefreshCw } from "@lucide/svelte";
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
  let confirmingRotate = $state(false);
  let rotating = $state(false);
  let errorMessage = $state("");

  let confirmTimer: ReturnType<typeof window.setTimeout> | undefined;

  async function copyApiKey() {
    errorMessage = "";
    try {
      await navigator.clipboard.writeText(apiKey);
      copied = true;
      window.setTimeout(() => {
        copied = false;
      }, 1800);
    } catch {
      errorMessage = $t("app.settings.apiKeyCopyFailed");
    }
  }

  // Rotation silently breaks every reader still holding the old key, so it takes a
  // second deliberate click rather than firing on the first one.
  function requestRotate() {
    errorMessage = "";
    confirmingRotate = true;
    window.clearTimeout(confirmTimer);
    confirmTimer = window.setTimeout(() => {
      confirmingRotate = false;
    }, 5000);
  }

  async function rotateApiKey() {
    window.clearTimeout(confirmTimer);
    confirmingRotate = false;
    rotating = true;
    errorMessage = "";
    try {
      const response = await fetch("/v1/settings/api-key", { method: "POST" });
      if (!response.ok) {
        throw new Error($t("app.settings.apiKeyCreateFailed"));
      }
      const { backend_api_key: created } = await response.json();
      settings.backend_api_key = created;
      revealed = true;
    } catch (caught) {
      errorMessage = caught instanceof Error ? caught.message : $t("app.settings.apiKeyCreateFailed");
    } finally {
      rotating = false;
    }
  }
</script>

<SettingsCard title={$t("app.settings.backendApi")} description={$t("app.settings.backendApiDescription")}>
  <SettingField
    id="backend-api-key"
    label={$t("app.settings.apiKey")}
    hint={$t("app.settings.apiKeyHint")}
  >
    <div class="flex flex-wrap items-center gap-2">
      <Input
        id="backend-api-key"
        type={revealed ? "text" : "password"}
        class="h-9 min-w-0 flex-1 font-mono"
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
      <Button
        variant={confirmingRotate ? "destructive" : "outline"}
        size="sm"
        type="button"
        disabled={rotating}
        onclick={confirmingRotate ? rotateApiKey : requestRotate}
      >
        {#if rotating}
          <Loader2 class="size-3.5 animate-spin" />
        {:else}
          <RefreshCw class="size-3.5" />
        {/if}
        {confirmingRotate ? $t("app.settings.apiKeyCreateConfirm") : $t("app.settings.apiKeyCreate")}
      </Button>
    </div>
  </SettingField>

  {#if errorMessage}
    <SettingsNotice>{errorMessage}</SettingsNotice>
  {/if}
</SettingsCard>
