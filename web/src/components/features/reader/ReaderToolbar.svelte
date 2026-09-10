<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { Card } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import { ArrowLeft } from "@lucide/svelte";

  let {
    downloaded = false,
    imageVersion = "best",
    onImageVersionChange,
    mode,
    onBack,
    onPageMode,
    onScrollMode,
    pageCountLabel,
  } = $props<{
    downloaded?: boolean;
    imageVersion?: "best" | "original";
    onImageVersionChange?: (value: "best" | "original") => void;
    mode: "scroll" | "page";
    onBack: () => void;
    onPageMode: () => void;
    onScrollMode: () => void;
    pageCountLabel: string;
  }>();
</script>

<Card class="sticky top-4 z-50 flex-row items-center justify-between px-5 py-3 backdrop-blur">
  <Button
    variant="ghost"
    size="sm"
    onclick={onBack}
    class="text-muted-foreground hover:text-foreground -ml-2"
  >
    <ArrowLeft class="w-4 h-4 mr-1" /> {$t("app.reader.back")}
  </Button>
  <div class="flex flex-wrap items-center justify-end gap-2 sm:gap-4">
    {#if downloaded}
      <select aria-label={$t("app.upscale.quality")} class="h-8 max-w-36 rounded-md border border-border bg-background px-2 text-xs" value={imageVersion} onchange={(event) => onImageVersionChange?.(event.currentTarget.value as "best" | "original")}>
        <option value="best">{$t("app.upscale.best")}</option>
        <option value="original">{$t("app.upscale.original")}</option>
      </select>
    {/if}
    <span class="text-xs font-medium text-muted-foreground">
      {pageCountLabel}
    </span>
    <div class="flex rounded-lg border border-border overflow-hidden p-0.5 bg-secondary/50">
      <Button
        variant={mode === "scroll" ? "default" : "ghost"}
        size="sm"
        class="h-7 text-xs px-3 rounded-md transition-all"
        onclick={onScrollMode}
      >
        {$t("app.reader.scroll")}
      </Button>
      <Button
        variant={mode === "page" ? "default" : "ghost"}
        size="sm"
        class="h-7 text-xs px-3 rounded-md transition-all"
        onclick={onPageMode}
      >
        {$t("app.reader.page")}
      </Button>
    </div>
  </div>
</Card>
