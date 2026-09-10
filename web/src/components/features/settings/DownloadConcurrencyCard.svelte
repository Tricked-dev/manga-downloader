<script lang="ts">
  import { Input } from "$lib/ui/input";
  import {
    getDownloadConcurrentChapters,
    getDownloadPageFetchConcurrency,
  } from "$lib/features/settings/utils";
  import { t } from "$lib/i18n";
  import SettingField from "./SettingField.svelte";
  import SettingsCard from "./SettingsCard.svelte";

  import type { SettingsMap } from "$lib/features/settings/types";

  let { settings = $bindable() } = $props<{ settings: SettingsMap }>();

  function normalizeConcurrency(value: string): string {
    const parsed = Number.parseInt(value, 10);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      return "1";
    }
    return String(Math.min(Math.floor(parsed), 32));
  }

  function updateConcurrentChapters(event: Event) {
    settings.download_concurrent_chapters = normalizeConcurrency(
      (event.currentTarget as HTMLInputElement).value,
    );
  }

  function updatePageFetchConcurrency(event: Event) {
    settings.download_page_fetch_concurrency = normalizeConcurrency(
      (event.currentTarget as HTMLInputElement).value,
    );
  }
</script>

<SettingsCard
  title={$t("app.settings.downloadConcurrency.title")}
  description={$t("app.settings.downloadConcurrency.description")}
>
  <SettingField
    id="download-concurrent-chapters"
    label={$t("app.settings.downloadConcurrency.concurrentChapters")}
    hint={$t("app.settings.downloadConcurrency.concurrentChaptersHint")}
  >
    <Input
      id="download-concurrent-chapters"
      type="number"
      min="1"
      step="1"
      max="32"
      value={String(getDownloadConcurrentChapters(settings))}
      oninput={updateConcurrentChapters}
      class="h-9 max-w-28"
    />
  </SettingField>

  <SettingField
    id="download-page-fetch-concurrency"
    label={$t("app.settings.downloadConcurrency.pageFetches")}
    hint={$t("app.settings.downloadConcurrency.pageFetchesHint")}
  >
    <Input
      id="download-page-fetch-concurrency"
      type="number"
      min="1"
      step="1"
      max="32"
      value={String(getDownloadPageFetchConcurrency(settings))}
      oninput={updatePageFetchConcurrency}
      class="h-9 max-w-28"
    />
  </SettingField>
</SettingsCard>
