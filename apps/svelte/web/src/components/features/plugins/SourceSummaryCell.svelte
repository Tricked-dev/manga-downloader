<script lang="ts">
  import { t } from "$lib/i18n";
  import type { Source } from "$lib/types";

  const {
    source,
    iconUrl = null,
    showKey = false,
    truncateBaseUrl = false,
  } = $props<{
    source: Source;
    iconUrl?: string | null;
    showKey?: boolean;
    truncateBaseUrl?: boolean;
  }>();
</script>

<div class={iconUrl ? "flex items-center gap-3" : "space-y-1"}>
  {#if iconUrl}
    <img
      src={iconUrl}
      alt=""
      aria-hidden="true"
      class="h-9 w-9 shrink-0 rounded-md border border-border/70 bg-muted object-cover"
      decoding="async"
      height="36"
      loading="lazy"
      width="36"
    />
  {/if}
  <div class="min-w-0 space-y-1">
    <p class="text-sm font-medium">{source.display_name}</p>
    {#if showKey}
      <p class="text-[11px] text-muted-foreground">{$t("app.sources.keyValue", { key: source.name })}</p>
    {/if}
    <div class="flex items-center gap-2 text-xs text-muted-foreground">
      <p class={truncateBaseUrl ? "truncate" : ""}>{source.base_url}</p>
    </div>
  </div>
</div>
