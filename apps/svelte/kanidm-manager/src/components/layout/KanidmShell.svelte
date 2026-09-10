<script lang="ts">
  import { BookOpen, Menu, X } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import type { KanidmManagerTab, KanidmNavItem } from "$lib/kanidm/types";

  let {
    activeTab,
    children,
    navItems,
    onSelect,
  }: {
    activeTab: KanidmManagerTab;
    children: Snippet;
    navItems: KanidmNavItem[];
    onSelect: (tab: KanidmManagerTab) => void;
  } = $props();

  let mobileOpen = $state(false);

  function closeMobileMenu() {
    mobileOpen = false;
  }

  function openMobileMenu() {
    mobileOpen = true;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      closeMobileMenu();
    }
  }

  function navButtonClass(selected: boolean) {
    return selected
      ? "flex w-full items-center gap-3 rounded-xl border border-primary/60 bg-card px-4 py-2.5 text-left text-sm font-medium text-foreground transition-colors"
      : "flex w-full items-center gap-3 rounded-xl border border-transparent px-4 py-2.5 text-left text-sm font-medium text-muted-foreground transition-colors hover:border-border hover:bg-card hover:text-foreground";
  }

  function selectTab(tab: KanidmManagerTab) {
    onSelect(tab);
    closeMobileMenu();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#snippet brand()}
  <div class="flex items-center gap-3">
    <div class="flex h-8 w-8 items-center justify-center rounded-xl border border-primary/40 bg-primary/10 text-primary">
      <BookOpen class="h-4 w-4" />
    </div>
    <div>
      <p class="text-base font-semibold text-foreground">Manga Server</p>
      <p class="text-[10px] uppercase tracking-normal text-muted-foreground">Kanidm Manager</p>
    </div>
  </div>
{/snippet}

{#snippet tabNavigation()}
  <nav class="flex flex-1 flex-col gap-4 p-3" aria-label="Kanidm manager sections">
    <div class="flex flex-col gap-1.5" role="tablist" aria-orientation="vertical">
      {#each navItems as item (item.id)}
        <button
          class={navButtonClass(activeTab === item.id)}
          type="button"
          role="tab"
          aria-selected={activeTab === item.id}
          aria-controls="kanidm-content"
          onclick={() => selectTab(item.id)}
        >
          <item.icon class="h-5 w-5 shrink-0" />
          <span class="min-w-0 flex-1 truncate">{item.label}</span>
          {#if item.count !== undefined}
            <span class="rounded-lg border border-border/70 bg-background/70 px-1.5 py-0.5 text-xs text-muted-foreground">
              {item.count}
            </span>
          {/if}
        </button>
      {/each}
    </div>
  </nav>
{/snippet}

<div class="min-h-screen bg-background lg:grid lg:grid-cols-[15rem_minmax(0,1fr)]">
  <a
    href="#content"
    class="sr-only focus:not-sr-only focus:absolute focus:left-4 focus:top-4 focus:z-50 focus:border focus:border-border focus:bg-background focus:px-3 focus:py-2"
  >
    Skip to content
  </a>

  <header class="sticky top-0 z-40 flex items-center justify-between border-b border-border/80 bg-background/95 px-4 py-4 backdrop-blur lg:hidden">
    {@render brand()}
    <button
      class="button button-outline h-9 w-9 px-0"
      type="button"
      aria-label="Open navigation"
      aria-expanded={mobileOpen}
      aria-controls="mobile-navigation"
      onclick={openMobileMenu}
    >
      <Menu class="h-5 w-5" />
    </button>
  </header>

  {#if mobileOpen}
    <div class="fixed inset-0 z-50 lg:hidden" role="dialog" aria-modal="true" aria-label="Kanidm manager navigation">
      <button class="absolute inset-0 bg-background/80 backdrop-blur-sm" type="button" aria-label="Close navigation" onclick={closeMobileMenu}></button>
      <aside
        id="mobile-navigation"
        class="relative flex h-full w-[min(21rem,calc(100vw-3rem))] flex-col border-r border-border bg-background shadow-xl"
      >
        <div class="flex items-center justify-between border-b border-border/80 px-4 py-4">
          {@render brand()}
          <button class="button button-ghost h-8 w-8 px-0" type="button" aria-label="Close navigation" onclick={closeMobileMenu}>
            <X class="h-5 w-5" />
          </button>
        </div>
        {@render tabNavigation()}
      </aside>
    </div>
  {/if}

  <aside class="hidden border-r border-border/80 bg-background/95 lg:sticky lg:top-0 lg:flex lg:h-screen lg:w-60 lg:flex-col lg:backdrop-blur">
    <div class="border-b border-border/80 px-4 py-4">
      {@render brand()}
    </div>
    {@render tabNavigation()}
  </aside>

  <div class="min-w-0">
    <main id="content" class="px-4 py-5 sm:px-6 lg:px-8 lg:py-7">
      <div class="mx-auto w-full max-w-7xl">
        {@render children()}
      </div>
    </main>
  </div>
</div>
