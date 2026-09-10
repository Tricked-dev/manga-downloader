<script lang="ts">
  import { Database, Trash2 } from "@lucide/svelte";

  import { Button } from "$lib/ui/button";
  import { Input } from "$lib/ui/input";
  import { Select, SelectContent, SelectItem, SelectTrigger } from "$lib/ui/select";
  import {
    encodeBinaryUnitSetting,
    parseBinaryUnitSetting,
  } from "$lib/features/settings/utils";
  import { t } from "$lib/i18n";
  import SettingField from "./SettingField.svelte";
  import SettingInfoHint from "./SettingInfoHint.svelte";
  import SettingsCard from "./SettingsCard.svelte";

  import type { SettingsMap } from "$lib/features/settings/types";

  const CACHE_MEMORY_DEFAULTS = {
    defaultAmount: "256",
    defaultUnit: "MiB",
  } as const;

  let {
    settings = $bindable(),
    clearingCache,
    clearCacheResult,
    onClearCache,
    cleaningDatabase,
    cleanupDatabaseResult,
    onCleanupDatabase,
  } = $props<{
    settings: SettingsMap;
    clearingCache: boolean;
    clearCacheResult: string;
    onClearCache: () => void;
    cleaningDatabase: boolean;
    cleanupDatabaseResult: string;
    onCleanupDatabase: () => void;
  }>();

  const cacheMemory = $derived(parseBinaryUnitSetting(settings.cache_max_memory_bytes, CACHE_MEMORY_DEFAULTS));

  function updateCacheMemoryAmount(value: string) {
    settings.cache_max_memory_bytes = encodeBinaryUnitSetting(value, cacheMemory.unit);
  }

  function updateCacheMemoryUnit(value: string) {
    settings.cache_max_memory_bytes = encodeBinaryUnitSetting(cacheMemory.amount, value === "GiB" ? "GiB" : "MiB");
  }

  function handleCacheMemoryAmountInput(event: Event) {
    updateCacheMemoryAmount((event.currentTarget as HTMLInputElement).value);
  }
</script>

<SettingsCard title={$t("app.settings.cache.title")} description={$t("app.settings.cache.description")}>
  <SettingField
    id="cache-path"
    label={$t("app.settings.cache.path")}
    hint={$t("app.settings.cache.pathHint")}
  >
    <Input
      id="cache-path"
      type="text"
      class="h-9 font-mono"
      bind:value={settings.cache_disk_path}
    />
  </SettingField>

  <SettingField
    id="cache-memory"
    label={$t("app.settings.cache.maxMemory")}
    hint={$t("app.settings.cache.maxMemoryHint")}
  >
    <div class="grid gap-2 sm:grid-cols-[minmax(0,1fr)_8rem]">
      <Input
        id="cache-memory"
        type="number"
        min="1"
        step="1"
        value={cacheMemory.amount}
        oninput={handleCacheMemoryAmountInput}
        class="h-9"
      />
      <Select type="single" value={cacheMemory.unit} onValueChange={updateCacheMemoryUnit}>
        <SelectTrigger class="h-9 w-full">
          {cacheMemory.unit}
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="MiB">MiB</SelectItem>
          <SelectItem value="GiB">GiB</SelectItem>
        </SelectContent>
      </Select>
    </div>
  </SettingField>

  <div class="pt-0.5">
    <div class="flex flex-wrap items-center gap-2">
      <Button
        id="clear-cache-button"
        variant="secondary"
        onclick={onClearCache}
        disabled={clearingCache || cleaningDatabase}
        class="h-9"
      >
        {#if clearingCache}
          {$t("app.settings.cache.clearing")}
        {:else}
          <Trash2 class="mr-2 h-4 w-4" /> {$t("app.settings.cache.clearCache")}
        {/if}
      </Button>
      <Button
        id="cleanup-database-button"
        variant="secondary"
        onclick={onCleanupDatabase}
        disabled={cleaningDatabase || clearingCache}
        class="h-9"
      >
        {#if cleaningDatabase}
          {$t("app.settings.cache.cleaning")}
        {:else}
          <Database class="mr-2 h-4 w-4" /> {$t("app.settings.cache.cleanDatabase")}
        {/if}
      </Button>
      <SettingInfoHint content={$t("app.settings.cache.hint")} />
    </div>
    {#if clearCacheResult}
      <p class="mt-2 text-sm text-muted-foreground">{clearCacheResult}</p>
    {/if}
    {#if cleanupDatabaseResult}
      <p class="mt-2 text-sm text-muted-foreground">{cleanupDatabaseResult}</p>
    {/if}
  </div>
</SettingsCard>
