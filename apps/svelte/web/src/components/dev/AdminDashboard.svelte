<script lang="ts">
  import { browser } from "$app/environment";
  import {
    AlertCircle,
    LayoutDashboard,
    RefreshCw,
    Shield,
    ShieldOff,
    Users,
    X,
  } from "@lucide/svelte";
  import { Button } from "$lib/ui/button";
  import { t } from "$lib/i18n";
  import { Tabs, TabsContent, TabsList, TabsTrigger } from "$lib/ui/tabs";
  import { cn } from "$lib/utils";
  import AdminOverviewPanel from "./AdminOverviewPanel.svelte";
  import AdminUsersPanel from "./AdminUsersPanel.svelte";
  import {
    loadDevAdminSummary,
    type AdminSummary,
    type SessionResponse,
  } from "./dev-api";

  type ActiveTab = "overview" | "users";

  const ADMIN_SCROLL_AREA_CLASS = [
    "min-h-0 flex-1 overflow-y-auto p-6",
    "[scrollbar-color:var(--border)_transparent]",
    "[scrollbar-gutter:stable]",
    "[scrollbar-width:thin]",
    "[&::-webkit-scrollbar]:w-3",
    "[&::-webkit-scrollbar-track]:bg-transparent",
    "[&::-webkit-scrollbar-thumb]:rounded-full",
    "[&::-webkit-scrollbar-thumb]:border-[3px]",
    "[&::-webkit-scrollbar-thumb]:border-background",
    "[&::-webkit-scrollbar-thumb]:border-solid",
    "[&::-webkit-scrollbar-thumb]:bg-border",
    "[&::-webkit-scrollbar-thumb:hover]:bg-muted-foreground/55",
  ].join(" ");

  let {
    open,
    onOpenChange,
    sessionInfo,
  }: {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    sessionInfo: SessionResponse | null;
  } = $props();

  let activeTab = $state<ActiveTab>("overview");
  let loading = $state(false);
  let errorMessage = $state<string | null>(null);
  let summary = $state<AdminSummary>({
    downloads: [],
    health: null,
    info: null,
    libraryCount: 0,
    settings: {},
    sources: [],
  });
  let searchQuery = $state("");
  let userPage = $state(1);

  const user = $derived(sessionInfo?.session?.user ?? null);
  const isAuthenticated = $derived(Boolean(sessionInfo?.authenticated));
  const authEnabled = $derived(Boolean(sessionInfo?.authEnabled));
  const isAdmin = $derived(!authEnabled || user?.role === "admin");
  const visibleUser = $derived.by(() => {
    if (!user) {
      return null;
    }

    const haystack = `${user.name ?? ""} ${user.email ?? ""} ${user.role ?? ""}`.toLowerCase();
    return haystack.includes(searchQuery.trim().toLowerCase()) ? user : null;
  });
  const totalUserPages = 1;
  const enabledSources = $derived(countEnabledSources());
  const settingsCount = $derived(countSettings());
  const failedDownloads = $derived(countFailedDownloads());
  const activeDownloads = $derived(countActiveDownloads());
  const healthOk = $derived(Boolean(summary.health?.ok));

  $effect(() => {
    if (!browser || !open || !isAdmin) {
      return;
    }

    const controller = new AbortController();
    void loadAdminSummary(controller.signal);

    return () => {
      controller.abort();
    };
  });

  function close(): void {
    onOpenChange(false);
  }

  function handleRefresh(): void {
    void loadAdminSummary();
  }

  function handleTabValueChange(value: string): void {
    activeTab = value === "users" ? "users" : "overview";
  }

  function handleSearchInput(event: Event): void {
    searchQuery = (event.currentTarget as HTMLInputElement).value;
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (open && event.key === "Escape") {
      close();
    }
  }

  async function loadAdminSummary(signal?: AbortSignal): Promise<void> {
    loading = true;
    errorMessage = null;

    const response = await loadDevAdminSummary(signal, $t);

    if (signal?.aborted) {
      return;
    }

    summary = response.summary;
    errorMessage = response.errorMessage;
    loading = false;
  }

  function countEnabledSources(): number {
    let count = 0;
    for (const source of summary.sources) {
      if (source.enabled) {
        count += 1;
      }
    }

    return count;
  }

  function countSettings(): number {
    let count = 0;
    for (const _key in summary.settings) {
      count += 1;
    }

    return count;
  }

  function countFailedDownloads(): number {
    let count = 0;
    for (const download of summary.downloads) {
      if (download.status === "failed" || download.status === "error") {
        count += 1;
      }
    }

    return count;
  }

  function countActiveDownloads(): number {
    let count = 0;
    for (const download of summary.downloads) {
      if (["queued", "running", "downloading", "processing"].includes(download.status ?? "")) {
        count += 1;
      }
    }

    return count;
  }

  function nextUserPage(): void {
    userPage = Math.min(totalUserPages, userPage + 1);
  }

  function previousUserPage(): void {
    userPage = Math.max(1, userPage - 1);
  }

</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div class="fixed inset-0 z-[60]">
    <button
      type="button"
      class="absolute inset-0 cursor-default bg-background/70 backdrop-blur-sm"
      aria-label={$t("app.dev.admin.closeDashboard")}
      onclick={close}
    ></button>

    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="admin-dashboard-title"
      class="absolute left-1/2 top-1/2 flex h-[min(85vh,760px)] w-[min(1200px,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden border border-border bg-background shadow-2xl"
    >
      {#if authEnabled && !isAdmin}
        <div class="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
          <div class="flex size-16 items-center justify-center rounded-lg bg-muted text-muted-foreground">
            <ShieldOff class="size-8" />
          </div>
          <div class="space-y-2">
            <h2 class="text-xl font-semibold">{$t("app.dev.admin.accessDenied")}</h2>
            <p class="max-w-sm text-sm text-muted-foreground">{$t("app.dev.admin.accessDeniedDescription")}</p>
          </div>
          <Button variant="outline" onclick={close}>{$t("app.actions.cancel")}</Button>
        </div>
      {:else}
        <header class="flex shrink-0 items-center justify-between border-b border-border bg-background px-6 py-4">
          <div class="flex min-w-0 items-center gap-3">
            <div class="flex size-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <Shield class="size-5" />
            </div>
            <div class="min-w-0">
              <h2 id="admin-dashboard-title" class="truncate text-lg font-semibold">{$t("app.dev.admin.title")}</h2>
              <p class="hidden text-sm text-muted-foreground sm:block">{$t("app.dev.admin.description")}</p>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <Button variant="outline" size="sm" disabled={loading} onclick={handleRefresh}>
              <RefreshCw class={cn("size-3.5", loading && "animate-spin")} />
              {$t("app.dev.admin.refresh")}
            </Button>
            <Button variant="ghost" size="icon" aria-label={$t("app.dev.admin.closeDashboard")} onclick={close}>
              <X class="size-4" />
            </Button>
          </div>
        </header>

        <Tabs value={activeTab} onValueChange={handleTabValueChange} class="min-h-0 flex-1 gap-0">
          <div class="shrink-0 border-b border-border px-6">
            <TabsList variant="line" class="h-auto gap-6">
              <TabsTrigger value="overview" class="px-2 pb-3.5 pt-4">
                <LayoutDashboard class="size-4" />
                {$t("app.dev.admin.overview")}
              </TabsTrigger>
              <TabsTrigger value="users" class="px-2 pb-3.5 pt-4">
                <Users class="size-4" />
                {$t("app.dev.admin.users")}
              </TabsTrigger>
            </TabsList>
          </div>

          <div class={ADMIN_SCROLL_AREA_CLASS}>
            {#if errorMessage}
              <div class="mb-4 flex items-center gap-3 border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                <AlertCircle class="size-4" />
                {errorMessage}
              </div>
            {/if}

            <TabsContent value="overview" class="mt-0">
              <AdminOverviewPanel
                {activeDownloads}
                {enabledSources}
                {failedDownloads}
                {healthOk}
                {loading}
                {settingsCount}
                {summary}
              />
            </TabsContent>

            <TabsContent value="users" class="mt-0">
              <AdminUsersPanel
                {authEnabled}
                {isAuthenticated}
                {nextUserPage}
                {previousUserPage}
                {searchQuery}
                {totalUserPages}
                {userPage}
                {visibleUser}
                onSearchInput={handleSearchInput}
              />
            </TabsContent>
          </div>
        </Tabs>
      {/if}
    </div>
  </div>
{/if}
