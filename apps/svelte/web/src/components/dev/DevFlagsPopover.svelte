<script lang="ts">
  import { Flag } from "@lucide/svelte";
  import { Badge } from "$lib/ui/badge";
  import { t } from "$lib/i18n";
  import { Popover, PopoverContent, PopoverTrigger } from "$lib/ui/popover";
  import { Switch } from "$lib/ui/switch";
  import type { SessionResponse } from "./dev-api";

  const flagItems = [
    { id: "auth", labelKey: "app.dev.toolbar.authBoundary" },
    { id: "query-devtools", labelKey: "app.dev.toolbar.queryDevtools" },
    { id: "toolbar", labelKey: "app.dev.toolbar.devToolbar" },
  ] as const;

  const {
    sessionError,
    sessionInfo,
    toolbarButtonClass,
  }: {
    sessionError: string | null;
    sessionInfo: SessionResponse | null;
    toolbarButtonClass: string;
  } = $props();

  const enabledFlagCount = $derived((sessionInfo?.authEnabled ? 1 : 0) + 2);

  function flagEnabled(flag: (typeof flagItems)[number]): boolean {
    return flag.id === "auth" ? (sessionInfo?.authEnabled ?? false) : true;
  }
</script>

<Popover>
  <PopoverTrigger class={toolbarButtonClass} type="button" title={$t("app.dev.toolbar.flagsTitle")}>
    <Flag class="size-3.5" />
    <span>{$t("app.dev.toolbar.flags")}</span>
    <Badge variant="secondary" class="ml-1 h-4 px-1.5 text-[9px]">{enabledFlagCount}/{flagItems.length}</Badge>
  </PopoverTrigger>
  <PopoverContent align="start" side="top" sideOffset={6} class="w-60 border border-border p-2 text-xs">
    <div class="px-2 pb-2 text-[11px] font-semibold uppercase text-muted-foreground">{$t("app.dev.toolbar.runtimeFlags")}</div>
    <div class="space-y-1">
      {#each flagItems as flag (flag.id)}
        <div class="flex items-center justify-between gap-3 px-2 py-1.5">
          <span class="flex items-center gap-2 text-muted-foreground">
            <span class={flagEnabled(flag) ? "size-2 rounded-full bg-primary" : "size-2 rounded-full bg-muted-foreground/30"}></span>
            {$t(flag.labelKey)}
          </span>
          <Switch checked={flagEnabled(flag)} disabled size="sm" />
        </div>
      {/each}
    </div>
    {#if sessionError}
      <div class="mt-2 border-t border-border px-2 pt-2 text-[11px] text-destructive">{sessionError}</div>
    {/if}
  </PopoverContent>
</Popover>
