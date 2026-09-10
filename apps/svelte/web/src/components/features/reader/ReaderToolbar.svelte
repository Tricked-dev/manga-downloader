<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { Card } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import { ArrowLeft } from "@lucide/svelte";

  let {
    mode,
    onBack,
    onPageMode,
    onScrollMode,
    pageCountLabel,
  } = $props<{
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
  <div class="flex items-center gap-4">
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
