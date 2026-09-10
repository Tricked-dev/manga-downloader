<script lang="ts">
  import { Input } from "$lib/ui/input";
  import SettingField from "./SettingField.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import SettingsNotice from "./SettingsNotice.svelte";
  import { t } from "$lib/i18n";

  import type { SettingsMap } from "$lib/features/settings/types";

  let { settings = $bindable() } = $props<{ settings: SettingsMap }>();

  const botConfigured = $derived(Boolean(settings.discord_bot_token?.trim()));
  const notificationsConfigured = $derived(Boolean(settings.discord_channel_id?.trim()));

  function setDiscordChannelId(value: string) {
    settings.discord_channel_id = value.replace(/\D/g, "");
  }

  function handleDiscordChannelInput(event: Event) {
    setDiscordChannelId((event.currentTarget as HTMLInputElement).value);
  }
</script>

<SettingsCard title={$t("app.settings.discord.title")} description={$t("app.settings.discord.description")}>
  <SettingField
    id="discord-bot-token"
    label={$t("app.settings.discord.botToken")}
    hint={$t("app.settings.discord.botTokenHint")}
  >
    <Input
      id="discord-bot-token"
      type="password"
      class="h-9"
      bind:value={settings.discord_bot_token}
      autocomplete="new-password"
      placeholder={$t("app.settings.discord.botTokenPlaceholder")}
    />
  </SettingField>

  <SettingField
    id="discord-channel-id"
    label={$t("app.settings.discord.channelId")}
    hint={$t("app.settings.discord.channelIdHint")}
  >
    <Input
      id="discord-channel-id"
      type="text"
      inputmode="numeric"
      pattern="[0-9]*"
      class="h-9 font-mono"
      value={settings.discord_channel_id}
      oninput={handleDiscordChannelInput}
      placeholder="123456789012345678"
    />
  </SettingField>

  {#if botConfigured && !notificationsConfigured}
    <SettingsNotice>
      {$t("app.settings.discord.notice")}
    </SettingsNotice>
  {/if}
</SettingsCard>
