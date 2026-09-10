<script lang="ts">
  import { Info } from "@lucide/svelte";
  import { buttonVariants } from "$lib/ui/button";
  import { Popover, PopoverContent, PopoverTrigger } from "$lib/ui/popover";
  import { Switch } from "$lib/ui/switch";
  import { t } from "$lib/i18n";
  import type { Source, SourceSettings } from "$lib/types";
  import { cn } from "$lib/utils";

  const INFO_TRIGGER_CLASS = cn(buttonVariants({ variant: "outline", size: "icon" }), "text-muted-foreground hover:text-foreground");

  const {
    source,
    open,
    setting,
    loadingSettings = false,
    savingSettings = false,
    onOpenChange,
    onHideNsfwChange,
  } = $props<{
    source: Source;
    open: boolean;
    setting?: SourceSettings;
    loadingSettings?: boolean;
    savingSettings?: boolean;
    onOpenChange: (open: boolean) => void;
    onHideNsfwChange: (checked: boolean) => void;
  }>();
</script>

<Popover {open} onOpenChange={onOpenChange}>
  <PopoverTrigger
    class={INFO_TRIGGER_CLASS}
    aria-label={$t("app.sources.showInfo", { source: source.display_name })}
  >
    <Info class="size-4" />
  </PopoverTrigger>
  <PopoverContent
    align="end"
    class="w-84 max-w-[min(28rem,calc(100vw-3rem))] px-3 py-3 text-left text-xs leading-5"
  >


    <div class="mt-3 space-y-1 text-muted-foreground">
      <p><span class="font-semibold text-foreground">{$t("app.sources.key")}</span> {source.name}</p>
      <p><span class="font-semibold text-foreground">{$t("app.sources.baseUrl")}</span> {source.base_url}</p>
      <p><span class="font-semibold text-foreground">{$t("app.sources.capabilities")}:</span> {source.capabilities.join(", ") || $t("app.sources.noneReported")}</p>
      {#if source.homepage}
        <p><span class="font-semibold text-foreground">{$t("app.sources.homepage")}</span> <a class="underline underline-offset-2" href={source.homepage} target="_blank" rel="noreferrer">{source.homepage}</a></p>
      {/if}
      {#if source.source_repository}
        <p><span class="font-semibold text-foreground">{$t("app.sources.repository")}</span> <a class="underline underline-offset-2" href={source.source_repository} target="_blank" rel="noreferrer">{source.source_repository}</a></p>
      {/if}
    </div>

    <div class="mt-3 rounded-md border border-border/70 px-3 py-2">
      <div class="flex items-start justify-between gap-3">
        <div>
          <p class="text-sm font-medium text-foreground">{$t("app.sources.hideNsfw")}</p>
          <p class="text-xs text-muted-foreground">{$t("app.sources.hideNsfwDescription")}</p>
        </div>

        {#if loadingSettings}
          <p class="text-xs text-muted-foreground">{$t("app.sources.loading")}</p>
        {:else}
          <Switch
            checked={setting?.hide_nsfw ?? false}
            disabled={savingSettings}
            onCheckedChange={onHideNsfwChange}
          />
        {/if}
      </div>
    </div>


  </PopoverContent>
</Popover>
