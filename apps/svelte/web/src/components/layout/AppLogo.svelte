<script lang="ts">
  import { BookOpen } from "@lucide/svelte";
  import { t } from "$lib/i18n";

  const { compact = false } = $props<{ compact?: boolean }>();
</script>

<div class="flex items-center gap-3">
  <div class="flex h-8 w-8 items-center justify-center border border-primary/40 bg-primary/10 text-primary">
    <BookOpen class="h-4 w-4" />
  </div>
  {#if !compact}
    <div>
      <p class="text-base font-semibold tracking-tight text-foreground">{$t("app.appLogo.title")}</p>
      <p class="text-[10px] uppercase tracking-[0.24em] text-muted-foreground">{$t("app.appLogo.subtitle")}</p>
    </div>
  {/if}
</div>
