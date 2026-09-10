<script lang="ts">
  import { preloadData } from "$app/navigation";
  import { resolve } from "$app/paths";
  import { page } from "$app/state";
  import {
    CircleHelp,
    ChartArea,
    Download,
    History,
    Library,
    Menu,
    MonitorSmartphone,
    Search,
    Settings,
    X,
  } from "@lucide/svelte";
  import { useQueryClient } from "@tanstack/svelte-query";
  import { SvelteSet } from "svelte/reactivity";
  import { t } from "$lib/i18n";
  import { Button } from "$lib/ui/button";
  import { QUERY_CACHE_TIMES } from "$lib/query-client";
  import AppLogo from "./AppLogo.svelte";
  import LanguagePicker from "./LanguagePicker.svelte";

  const queryClient = useQueryClient();
  const prefetchedRoutes = new SvelteSet<string>();

  const navItems = [
    { href: "/", icon: Library, labelKey: "app.nav.library" },
    { href: "/updates", icon: History, labelKey: "app.nav.updates" },
    { href: "/sources", icon: Search, labelKey: "app.nav.sources" },
    { href: "/downloads", icon: Download, labelKey: "app.nav.downloads" },
    { href: "/clients", icon: MonitorSmartphone, labelKey: "app.nav.clients" },
    { href: "/stats", icon: ChartArea, labelKey: "app.nav.stats" },
    { href: "/about", icon: CircleHelp, labelKey: "app.nav.about" },
    { href: "/settings", icon: Settings, labelKey: "app.nav.settings" },
  ] as const;

  function isActive(href: string, pathname: string) {
    return pathname === href || (href !== "/" && pathname.startsWith(href));
  }

  let mobileOpen = $state(false);

  function openMobileMenu() {
    mobileOpen = true;
  }

  function closeMobileMenu() {
    mobileOpen = false;
  }

  async function prefetchAppRoute(href: string) {
    if (isActive(href, page.url.pathname)) {
      return;
    }

    if (prefetchedRoutes.has(href)) {
      return;
    }

    prefetchedRoutes.add(href);
    void preloadData(resolve(href as "/" | "/stats" | "/updates" | "/sources" | "/downloads" | "/clients" | "/about" | "/settings"));

    switch (href) {
      case "/": {
        const { getGetLibraryQueryOptions } = await import("@manga-server/api-client/generated/endpoints/library");
        void queryClient.prefetchQuery(
          getGetLibraryQueryOptions({ query: { staleTime: QUERY_CACHE_TIMES.stable } }),
        );
        break;
      }
      case "/downloads": {
        const { getGetDownloadsQueryOptions } = await import("@manga-server/api-client/generated/endpoints/downloads");
        void queryClient.prefetchQuery(
          getGetDownloadsQueryOptions(undefined, { query: { staleTime: QUERY_CACHE_TIMES.active } }),
        );
        break;
      }
      case "/stats": {
        const { getGetStatsOverviewQueryOptions } = await import("@manga-server/api-client/generated/endpoints/stats");
        void queryClient.prefetchQuery(
          getGetStatsOverviewQueryOptions({ query: { staleTime: QUERY_CACHE_TIMES.active } }),
        );
        break;
      }
      case "/settings": {
        const [{ getGetSettingsQueryOptions }, { getListSourcesQueryOptions }] = await Promise.all([
          import("@manga-server/api-client/generated/endpoints/settings"),
          import("@manga-server/api-client/generated/endpoints/sources"),
        ]);
        void queryClient.prefetchQuery(
          getGetSettingsQueryOptions({ query: { staleTime: QUERY_CACHE_TIMES.stable } }),
        );
        void queryClient.prefetchQuery(
          getListSourcesQueryOptions({ query: { staleTime: QUERY_CACHE_TIMES.stable } }),
        );
        break;
      }
      case "/sources": {
        const { getListSourcesQueryOptions } = await import("@manga-server/api-client/generated/endpoints/sources");
        void queryClient.prefetchQuery(
          getListSourcesQueryOptions({ query: { staleTime: QUERY_CACHE_TIMES.stable } }),
        );
        break;
      }
      case "/updates": {
        const {
          getGetLibraryQueryOptions,
          getGetLibraryUpdatesQueryOptions,
        } = await import("@manga-server/api-client/generated/endpoints/library");
        void queryClient.prefetchQuery(
          getGetLibraryQueryOptions({ query: { staleTime: QUERY_CACHE_TIMES.stable } }),
        );
        void queryClient.prefetchQuery(
          getGetLibraryUpdatesQueryOptions({ query: { staleTime: QUERY_CACHE_TIMES.live } }),
        );
        break;
      }
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      closeMobileMenu();
    }
  }

  function prefetchRouteFromEvent(event: Event) {
    const href = (event.currentTarget as HTMLAnchorElement).dataset.href;
    if (href) {
      void prefetchAppRoute(href);
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<header class="sticky top-0 z-40 flex items-center justify-between border-b border-border/80 bg-background/95 px-4 py-4 backdrop-blur transition-shadow duration-150 lg:hidden">
  <a href={resolve("/")} class="inline-flex" aria-label={$t("app.aria.goToLibrary")} onclick={closeMobileMenu}>
    <AppLogo />
  </a>
  <Button variant="outline" size="icon-lg" aria-label={$t("app.aria.openNavigation")} aria-expanded={mobileOpen} aria-controls="mobile-navigation" onclick={openMobileMenu}>
    <Menu class="h-5 w-5" />
  </Button>
</header>

{#if mobileOpen}
  <div class="fixed inset-0 z-50 lg:hidden" role="dialog" aria-modal="true" aria-label={$t("app.aria.primaryNavigation")}>
    <button class="motion-fade-in absolute inset-0 bg-background/80 backdrop-blur-sm" type="button" aria-label={$t("app.aria.closeNavigation")} onclick={closeMobileMenu}></button>
    <aside
      id="mobile-navigation"
      class="motion-slide-in-left relative flex h-full w-[min(20rem,calc(100vw-3rem))] flex-col border-r border-border bg-background shadow-xl"
    >
      <div class="flex items-center justify-between border-b border-border/80 px-4 py-4">
        <a href={resolve("/")} class="inline-flex" aria-label={$t("app.aria.goToLibrary")} onclick={closeMobileMenu}>
          <AppLogo />
        </a>
        <Button variant="ghost" size="icon" aria-label={$t("app.aria.closeNavigation")} onclick={closeMobileMenu}>
          <X class="h-5 w-5" />
        </Button>
      </div>
      <nav class="flex flex-1 flex-col gap-1.5 overflow-y-auto p-3" aria-label={$t("app.aria.primaryNavigation")} data-sveltekit-preload-code="viewport">
        {#each navItems as item (item.href)}
          <a
            href={resolve(item.href)}
            data-href={item.href}
            aria-current={isActive(item.href, page.url.pathname) ? "page" : undefined}
            onclick={closeMobileMenu}
            onfocus={prefetchRouteFromEvent}
            onpointerenter={prefetchRouteFromEvent}
            class={isActive(item.href, page.url.pathname)
              ? "motion-interactive flex items-center gap-3 border border-primary/60 bg-card px-4 py-2.5 text-sm font-medium text-foreground"
              : "motion-interactive flex items-center gap-3 border border-transparent px-4 py-2.5 text-sm font-medium text-muted-foreground hover:translate-x-0.5 hover:border-border hover:bg-card hover:text-foreground"}
          >
            <item.icon class="h-5 w-5" />
            {$t(item.labelKey)}
          </a>
        {/each}
      </nav>
      <LanguagePicker />
    </aside>
  </div>
{/if}

<aside class="hidden border-r border-border/80 bg-background/95 lg:sticky lg:top-0 lg:flex lg:h-screen lg:w-60 lg:flex-col lg:backdrop-blur">
  <nav class="flex flex-1 flex-col gap-1.5 p-3" aria-label={$t("app.aria.primaryNavigation")} data-sveltekit-preload-code="viewport">
    {#each navItems as item (item.href)}
      <a
        href={resolve(item.href)}
        data-href={item.href}
        aria-current={isActive(item.href, page.url.pathname) ? "page" : undefined}
        onfocus={prefetchRouteFromEvent}
        onpointerenter={prefetchRouteFromEvent}
        class={isActive(item.href, page.url.pathname)
          ? "motion-interactive flex items-center gap-3 border border-primary/60 bg-card px-4 py-2.5 text-sm font-medium text-foreground"
          : "motion-interactive flex items-center gap-3 border border-transparent px-4 py-2.5 text-sm font-medium text-muted-foreground hover:translate-x-0.5 hover:border-border hover:bg-card hover:text-foreground"}
      >
        <item.icon class="h-5 w-5" />
        {$t(item.labelKey)}
      </a>
    {/each}
  </nav>
  <LanguagePicker />
</aside>
