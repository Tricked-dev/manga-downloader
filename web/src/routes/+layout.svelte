<script lang="ts">
  import "../app.css";
  import { browser } from "$app/environment";
  import { HydrationBoundary, QueryClientProvider } from "@tanstack/svelte-query";
  import type { DehydratedState } from "@tanstack/svelte-query";
  import { locale, t } from "$lib/i18n";
  import {
    QUERY_CACHE_BUSTER,
    QUERY_CACHE_STORAGE_KEY,
    QUERY_CACHE_TIMES,
    createAppQueryClient,
    getBrowserAppQueryClient,
  } from "$lib/query-client";
  import type { Component } from "svelte";

  const { children, data } = $props<{
    children: () => unknown;
    data?: {
      dehydratedState?: DehydratedState;
    };
  }>();
  const queryClient = browser
    ? getBrowserAppQueryClient()
    : createAppQueryClient({ observerQueriesEnabled: false });
  let QueryDevtools = $state<Component | null>(null);
  let DevToolbar = $state<Component | null>(null);

  if (browser && import.meta.env.DEV) {
    void import("@tanstack/svelte-query-devtools").then(({ SvelteQueryDevtools }) => {
      QueryDevtools = SvelteQueryDevtools;
    });
    void import("$components/dev/DevToolbar.svelte").then((module) => {
      DevToolbar = module.default;
    });
  }

  if (browser) {
    $effect(() => {
      document.documentElement.lang = $locale;
    });

    const restoreCachedQueries = () => {
      void Promise.all([
        import("@tanstack/query-persist-client-core"),
        import("@tanstack/query-async-storage-persister"),
      ]).then(([{ persistQueryClient }, { createAsyncStoragePersister }]) => {
        persistQueryClient({
          buster: QUERY_CACHE_BUSTER,
          maxAge: QUERY_CACHE_TIMES.inactive,
          persister: createAsyncStoragePersister({
            key: QUERY_CACHE_STORAGE_KEY,
            storage: {
              getItem: (key) => Promise.resolve(sessionStorage.getItem(key)),
              removeItem: (key) => {
                sessionStorage.removeItem(key);
                return Promise.resolve();
              },
              setItem: (key, value) => {
                sessionStorage.setItem(key, value);
                return Promise.resolve();
              },
            },
          }),
          queryClient,
        });
      });
    };

    if ("requestIdleCallback" in window) {
      window.requestIdleCallback(restoreCachedQueries, { timeout: 2000 });
    } else {
      setTimeout(restoreCachedQueries, 0);
    }
  }
</script>

<svelte:head>
  <title>{$t("app.meta.title")}</title>
  <meta name="description" content={$t("app.meta.description")} />
</svelte:head>

<QueryClientProvider client={queryClient}>
  {#if data?.dehydratedState}
    <HydrationBoundary state={data.dehydratedState} {queryClient} options={undefined}>
      {@render children()}
    </HydrationBoundary>
  {:else}
    {@render children()}
  {/if}
  {#if QueryDevtools}
    <QueryDevtools />
  {/if}
  {#if DevToolbar}
    <DevToolbar />
  {/if}
</QueryClientProvider>
