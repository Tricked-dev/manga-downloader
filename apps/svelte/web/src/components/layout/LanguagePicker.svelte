<script lang="ts">
  import { Globe2 } from "@lucide/svelte";
  import { locale, localeOptions, switchLocale, t } from "$lib/i18n";
  import { Select, SelectContent, SelectItem, SelectTrigger } from "$lib/ui/select";
  import ThemeToggle from "./ThemeToggle.svelte";

  function handleLocaleChange(value: string) {
    void switchLocale(value);
  }
</script>

<div class="border-t border-border/80 p-3">
  <div class="flex items-center gap-2">
    <Select type="single" value={$locale} onValueChange={handleLocaleChange}>
      <SelectTrigger class="h-9 min-w-0 flex-1 justify-start gap-2 overflow-hidden" aria-label={$t("app.locale.label")}>
        <Globe2 class="h-4 w-4 text-muted-foreground" />
        <span class="min-w-0 flex-1 truncate text-left">{$t(`app.locale.options.${$locale}`)}</span>
      </SelectTrigger>
      <SelectContent>
        {#each localeOptions as option (option.value)}
          <SelectItem value={option.value}>{$t(option.labelKey)}</SelectItem>
        {/each}
      </SelectContent>
    </Select>
    <ThemeToggle />
  </div>
</div>
