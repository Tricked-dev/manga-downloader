<script lang="ts">
  import { t } from "$lib/i18n";

  const { label = "", compact = false } = $props<{ label?: string; compact?: boolean }>();
  const resolvedLabel = $derived(label || $t("app.common.loading"));
</script>

<div class={`flex items-center justify-center ${compact ? "py-16" : "py-32"}`}>
  <div class="motion-panel flex items-center gap-3 border border-border/70 bg-card px-4 py-3 text-sm text-muted-foreground">
    <div class="h-6 w-6 animate-spin rounded-full border-2 border-muted border-t-primary"></div>
    <span>{resolvedLabel}</span>
  </div>
</div>
