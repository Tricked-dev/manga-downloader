<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { Input } from "$lib/ui/input";
  import SettingField from "./SettingField.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import SettingInfoHint from "./SettingInfoHint.svelte";
  import { t } from "$lib/i18n";

  const {
    getAvifConversionWorkers,
    setAvifConversionWorkers,
    reencoding,
    reencodeResult,
    onReencode,
  }: {
    getAvifConversionWorkers: () => number;
    setAvifConversionWorkers: (workers: number) => void;
    reencoding: boolean;
    reencodeResult: string;
    onReencode: () => void;
  } = $props();

  function handleWorkersInput(event: Event) {
    setAvifConversionWorkers(Number((event.currentTarget as HTMLInputElement).value));
  }
</script>

<SettingsCard title={$t("app.settings.avif.title")} description={$t("app.settings.avif.description")}>
  <SettingField
    id="avif-conversion-workers"
    label={$t("app.settings.avif.parallelConversions")}
    hint={$t("app.settings.avif.parallelConversionsHint")}
    class="space-y-2.5 rounded-md border border-border/70 bg-card/50 p-3.5"
  >
    <div class="flex items-center gap-3">
      <Input
        id="avif-conversion-workers"
        type="number"
        min="1"
        step="1"
        max="32"
        value={String(getAvifConversionWorkers())}
        oninput={handleWorkersInput}
        class="max-w-28"
      />
      <span class="text-sm text-muted-foreground">{$t("app.settings.avif.currentWorkers", { count: getAvifConversionWorkers() })}</span>
    </div>
  </SettingField>

  <div class="rounded-md border border-border/70 bg-card/50 p-3.5">
    <div class="flex items-center gap-2">
      <p class="text-sm font-medium">{$t("app.settings.avif.reencodeExisting")}</p>
      <SettingInfoHint content={$t("app.settings.avif.reencodeDescription")} />
    </div>
    <div class="mt-2.5">
      <Button
        variant="secondary"
        onclick={onReencode}
        disabled={reencoding}
        class="h-9"
      >
        {reencoding ? $t("app.settings.avif.reencoding") : $t("app.settings.avif.reencode")}
      </Button>
      {#if reencodeResult}
        <p class="mt-2 text-sm text-muted-foreground">{reencodeResult}</p>
      {/if}
    </div>
  </div>
</SettingsCard>
