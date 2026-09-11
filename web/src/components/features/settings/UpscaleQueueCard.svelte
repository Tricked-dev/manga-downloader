<script lang="ts">
  import { Loader2, Pause, Play } from "@lucide/svelte";
  import { Button } from "$lib/ui/button";
  import { Input } from "$lib/ui/input";
  import SettingField from "./SettingField.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import SettingsNotice from "./SettingsNotice.svelte";
  import { t } from "$lib/i18n";

  import type { SettingsMap } from "$lib/features/settings/types";

  let { settings = $bindable() } = $props<{ settings: SettingsMap }>();

  const paused = $derived(settings.upscale_paused === "true");

  let busy = $state(false);
  let errorMessage = $state("");

  function normalizeAutoResumeMinutes(value: string): string {
    const parsed = Number.parseInt(value, 10);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      return "0";
    }
    return String(Math.min(parsed, 1440));
  }

  function updateAutoResumeMinutes(event: Event) {
    settings.upscale_auto_resume_minutes = normalizeAutoResumeMinutes(
      (event.currentTarget as HTMLInputElement).value,
    );
  }

  async function setPaused(next: boolean) {
    busy = true;
    errorMessage = "";
    try {
      const response = await fetch(`/v1/upscale/${next ? "pause" : "resume"}`, { method: "POST" });
      if (!response.ok) {
        throw new Error($t("app.settings.upscaleQueueFailed"));
      }
      settings.upscale_paused = next ? "true" : "false";
    } catch (caught) {
      errorMessage = caught instanceof Error ? caught.message : $t("app.settings.upscaleQueueFailed");
    } finally {
      busy = false;
    }
  }
</script>

<SettingsCard
  title={$t("app.settings.upscaleQueue")}
  description={$t("app.settings.upscaleQueueDescription")}
>
  <SettingField
    id="upscale-paused"
    label={$t("app.settings.upscaleQueueState")}
    hint={$t("app.settings.upscaleQueueHint")}
  >
    <div class="flex items-center gap-3">
      <Button
        id="upscale-paused"
        variant="outline"
        size="sm"
        type="button"
        disabled={busy}
        onclick={() => setPaused(!paused)}
      >
        {#if busy}
          <Loader2 class="size-3.5 animate-spin" />
        {:else if paused}
          <Play class="size-3.5" />
        {:else}
          <Pause class="size-3.5" />
        {/if}
        {paused ? $t("app.settings.upscaleQueueResume") : $t("app.settings.upscaleQueuePause")}
      </Button>
      <span class="text-xs text-muted-foreground">
        {paused ? $t("app.settings.upscaleQueuePaused") : $t("app.settings.upscaleQueueRunning")}
      </span>
    </div>
  </SettingField>

  <SettingField
    id="upscale-auto-resume-minutes"
    label={$t("app.settings.upscaleQueueAutoResume")}
    hint={$t("app.settings.upscaleQueueAutoResumeHint")}
  >
    <Input
      id="upscale-auto-resume-minutes"
      type="number"
      min="0"
      step="1"
      max="1440"
      value={settings.upscale_auto_resume_minutes ?? "0"}
      oninput={updateAutoResumeMinutes}
      class="h-9 max-w-28"
    />
  </SettingField>

  {#if errorMessage}
    <SettingsNotice>{errorMessage}</SettingsNotice>
  {/if}
</SettingsCard>
