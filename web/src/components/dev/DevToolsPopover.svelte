<script lang="ts">
  import { resolve } from "$app/paths";
  import {
    Activity,
    CircleGauge,
    ExternalLink,
    LogOut,
    Moon,
    Server,
    Settings,
    Shield,
    Sun,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import type { ThemePreference } from "$lib/theme.svelte";
  import { Popover, PopoverContent, PopoverTrigger } from "$lib/ui/popover";
  import { cn } from "$lib/utils";
  import type { SessionResponse } from "./dev-api";

  interface DevToolLink {
    external?: boolean;
    href: string;
    id: string;
    labelKey: string;
  }

  const resolvePath = resolve as unknown as (route: string) => string;
  const toolLinks: readonly DevToolLink[] = [
    { id: "settings", labelKey: "app.nav.settings", href: "/settings" },
    { id: "about", labelKey: "app.dev.toolbar.buildInfo", href: "/about" },
    { id: "session", labelKey: "app.dev.toolbar.sessionJson", href: "/auth/session" },
    { id: "backend-health", labelKey: "app.dev.admin.backendHealth", href: "http://localhost:4000/v1/health", external: true },
    { id: "grafana", labelKey: "app.dev.toolbar.grafana", href: "http://localhost:3000", external: true },
  ] as const;

  const themeOptions = [
    { value: "light", labelKey: "app.dev.toolbar.light", icon: Sun },
    { value: "dark", labelKey: "app.dev.toolbar.dark", icon: Moon },
    { value: "system", labelKey: "app.dev.toolbar.system", icon: CircleGauge },
  ] as const;

  const {
    applyTheme,
    sessionInfo,
    signOut,
    theme,
    toolbarButtonClass,
  }: {
    applyTheme: (preference: ThemePreference) => void;
    sessionInfo: SessionResponse | null;
    signOut: () => void | Promise<void>;
    theme: ThemePreference;
    toolbarButtonClass: string;
  } = $props();

  function applyThemeFromButton(event: MouseEvent): void {
    const value = (event.currentTarget as HTMLButtonElement).value;
    if (value === "light" || value === "dark" || value === "system") {
      applyTheme(value);
    }
  }

  function displayHref(link: DevToolLink): string {
    return link.external ? link.href : resolvePath(link.href);
  }
</script>

<Popover>
  <PopoverTrigger class={toolbarButtonClass} type="button" title={$t("app.dev.toolbar.toolsTitle")}>
    <Settings class="size-3.5" />
    <span>{$t("app.dev.toolbar.tools")}</span>
  </PopoverTrigger>
  <PopoverContent align="start" side="top" sideOffset={6} class="w-64 border border-border p-2 text-xs">
    <div class="px-2 pb-2 text-[11px] font-semibold uppercase text-muted-foreground">{$t("app.dev.toolbar.quickLinks")}</div>
    <div class="space-y-1">
      {#each toolLinks as link (link.id)}
        <a
          href={displayHref(link)}
          target={link.external ? "_blank" : undefined}
          rel={link.external ? "noreferrer" : undefined}
          class="group flex items-center gap-2 px-2 py-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        >
          {#if link.id === "backend-health"}
            <Activity class="size-3.5" />
          {:else if link.external}
            <Server class="size-3.5" />
          {:else}
            <Shield class="size-3.5" />
          {/if}
          <span class="flex-1">{$t(link.labelKey)}</span>
          {#if link.external}
            <ExternalLink class="size-3 opacity-0 transition-opacity group-hover:opacity-100" />
          {/if}
        </a>
      {/each}
    </div>

    <div class="my-2 h-px bg-border"></div>

    <div class="px-2 pb-2 text-[11px] font-semibold uppercase text-muted-foreground">{$t("app.dev.toolbar.theme")}</div>
    <div class="space-y-1">
      {#each themeOptions as option (option.value)}
        <button
          type="button"
          value={option.value}
          class={cn(
            "flex w-full items-center gap-2 px-2 py-1.5 text-left text-muted-foreground transition-colors hover:bg-muted hover:text-foreground",
            theme === option.value && "bg-muted text-foreground",
          )}
          onclick={applyThemeFromButton}
        >
          <option.icon class="size-3.5" />
          {$t(option.labelKey)}
        </button>
      {/each}
    </div>

    {#if sessionInfo?.authenticated}
      <div class="my-2 h-px bg-border"></div>
      <button
        type="button"
        class="flex w-full items-center gap-2 px-2 py-1.5 text-left text-destructive transition-colors hover:bg-destructive/10"
        onclick={signOut}
      >
        <LogOut class="size-3.5" />
        {$t("app.dev.toolbar.signOut")}
      </button>
    {/if}
  </PopoverContent>
</Popover>
