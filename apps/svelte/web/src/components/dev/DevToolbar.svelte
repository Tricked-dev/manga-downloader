<script lang="ts">
  import { browser } from "$app/environment";
  import { resolve } from "$app/paths";
  import { page } from "$app/state";
  import {
    Activity,
    Check,
    Copy,
    LogIn,
    Moon,
    Shield,
    Sun,
    User,
    Zap,
  } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { Badge } from "$lib/ui/badge";
  import { t } from "$lib/i18n";
  import { themeController } from "$lib/theme.svelte";
  import { cn } from "$lib/utils";
  import AdminDashboard from "./AdminDashboard.svelte";
  import DevFlagsPopover from "./DevFlagsPopover.svelte";
  import DevToolsPopover from "./DevToolsPopover.svelte";
  import {
    loadDevSession,
    signOutDevSession,
    type SessionResponse,
  } from "./dev-api";

  const toolbarButtonClass =
    "inline-flex h-full shrink-0 items-center gap-2 px-3 text-xs text-muted-foreground transition-colors hover:bg-muted/70 hover:text-foreground focus-visible:bg-muted focus-visible:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50";

  let sessionInfo = $state<SessionResponse | null>(null);
  let sessionError = $state<string | null>(null);
  let sessionLoading = $state(true);
  let copiedKey = $state<string | null>(null);
  let collapsed = $state(false);
  let adminOpen = $state(false);
  let copyResetTimer: ReturnType<typeof setTimeout> | undefined;

  const user = $derived(sessionInfo?.session?.user ?? null);
  const userName = $derived(user?.name ?? user?.email ?? (sessionInfo?.authEnabled ? $t("app.dev.toolbar.guest") : $t("app.dev.toolbar.authDisabled")));
  const authStatus = $derived(
    sessionInfo?.authEnabled ? (sessionInfo.authenticated ? $t("app.dev.toolbar.signedIn") : $t("app.dev.toolbar.guest")) : $t("app.dev.toolbar.authOff"),
  );
  onMount(() => {
    themeController.initialize();
  });

  $effect(() => {
    if (!browser) {
      return;
    }

    const controller = new AbortController();
    void loadSession(controller.signal);

    return () => {
      controller.abort();
    };
  });

  async function loadSession(signal?: AbortSignal): Promise<void> {
    sessionLoading = true;
    sessionError = null;

    try {
      sessionInfo = await loadDevSession(signal, $t);
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") {
        return;
      }

      sessionError = error instanceof Error ? error.message : $t("app.dev.toolbar.sessionRequestFailedGeneric");
      sessionInfo = null;
    } finally {
      if (!signal?.aborted) {
        sessionLoading = false;
      }
    }
  }

  async function signOut(): Promise<void> {
    if (await signOutDevSession()) {
      await loadSession(undefined);
    }
  }

  async function copyText(key: string, text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.style.position = "fixed";
      textarea.style.opacity = "0";
      document.body.append(textarea);
      textarea.select();
      document.execCommand("copy");
      textarea.remove();
    }

    copiedKey = key;
    if (copyResetTimer) {
      clearTimeout(copyResetTimer);
    }
    copyResetTimer = setTimeout(() => {
      copiedKey = null;
    }, 1800);
  }

  function openAdminDashboard(): void {
    adminOpen = true;
  }

  function copyCurrentRoute(): void {
    void copyText("route", page.url.pathname);
  }

  function handleAdminOpenChange(open: boolean): void {
    adminOpen = open;
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key.toLowerCase() !== "d" || !event.shiftKey || !(event.metaKey || event.ctrlKey)) {
      return;
    }

    event.preventDefault();
    collapsed = !collapsed;
  }

</script>

<svelte:window onkeydown={handleKeydown} />

{#if !collapsed}
  <div
    role="toolbar"
    aria-label={$t("app.dev.toolbar.label")}
    class="fixed inset-x-0 bottom-0 z-50 flex h-8 items-center overflow-x-auto border-t border-border bg-background/95 font-mono text-xs shadow-lg backdrop-blur supports-[backdrop-filter]:bg-background/85"
  >
    <div class="flex h-full shrink-0 items-center px-2">
      <Badge variant="destructive" class="h-4 gap-1 px-1.5 text-[9px] font-bold uppercase">
        <Zap class="size-2.5" />
        {$t("app.dev.toolbar.dev")}
      </Badge>
    </div>

    <div class="h-4 w-px shrink-0 bg-border"></div>

    {#if sessionInfo?.authenticated}
      <a
        href={resolve("/settings")}
        class={cn(toolbarButtonClass, "text-primary")}
        title={user?.email ?? userName}
      >
        <User class="size-3.5" />
        <span class="max-w-36 truncate">{userName}</span>
      </a>
    {:else}
      <a href={resolve("/login")} class={toolbarButtonClass} title={authStatus}>
        <User class="size-3.5" />
        <span>{sessionLoading ? $t("app.dev.toolbar.checking") : userName}</span>
        {#if sessionInfo?.authEnabled}
          <LogIn class="size-3" />
        {/if}
      </a>
    {/if}

    <div class="h-4 w-px shrink-0 bg-border"></div>

    <DevFlagsPopover {sessionError} {sessionInfo} {toolbarButtonClass} />

    <div class="h-4 w-px shrink-0 bg-border"></div>

    <DevToolsPopover applyTheme={themeController.apply} {sessionInfo} {signOut} theme={themeController.preference} {toolbarButtonClass} />

    <div class="h-4 w-px shrink-0 bg-border"></div>

    <button
      type="button"
      class={cn(toolbarButtonClass, "text-amber-500")}
      title={$t("app.dev.toolbar.openAdmin")}
      onclick={openAdminDashboard}
    >
      <Shield class="size-3.5" />
      <span>{$t("app.dev.toolbar.admin")}</span>
    </button>

    <div class="h-4 w-px shrink-0 bg-border"></div>

    <button
      type="button"
      class={cn(toolbarButtonClass, "group")}
      title={$t("app.dev.toolbar.copyRoute")}
      onclick={copyCurrentRoute}
    >
      <Activity class="size-3.5 text-primary" />
      <span class="max-w-48 truncate">{page.url.pathname}</span>
      {#if copiedKey === "route"}
        <Check class="size-3 text-primary" />
      {:else}
        <Copy class="size-3 opacity-0 transition-opacity group-hover:opacity-100" />
      {/if}
    </button>

    <div class="min-w-4 flex-1"></div>

    <button
      type="button"
      class="inline-flex h-full shrink-0 items-center px-3 text-muted-foreground transition-colors hover:bg-muted/70 hover:text-foreground focus-visible:bg-muted focus-visible:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
      aria-label={$t("app.dev.toolbar.switchTheme", { theme: themeController.resolved === "dark" ? $t("app.dev.toolbar.light") : $t("app.dev.toolbar.dark") })}
      title={$t("app.dev.toolbar.toggleTheme")}
      onclick={themeController.toggle}
    >
      {#if themeController.resolved === "dark"}
        <Moon class="size-3.5" />
      {:else}
        <Sun class="size-3.5" />
      {/if}
    </button>
  </div>
{/if}

<AdminDashboard open={adminOpen} onOpenChange={handleAdminOpenChange} {sessionInfo} />
