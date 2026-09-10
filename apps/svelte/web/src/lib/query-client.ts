import type { Query, QueryKey } from "@tanstack/query-core";
import { QueryClient } from "@tanstack/svelte-query";

interface QueryClientOptions {
  observerQueriesEnabled?: boolean;
}

export const QUERY_CACHE_TIMES = {
  active: 60_000,
  inactive: 30 * 60_000,
  live: 30_000,
  stable: 5 * 60_000,
} as const;

export const QUERY_CACHE_BUSTER = "manga-server-query-cache-v2";
export const QUERY_CACHE_STORAGE_KEY = "manga-server:query-cache";

export function isFullDownloadsQueryKey(queryKey: QueryKey): boolean {
  return (
    (queryKey.length === 2 && queryKey[0] === "v1" && queryKey[1] === "downloads") ||
    (queryKey.length === 3 &&
      queryKey[0] === "infinite" &&
      queryKey[1] === "v1" &&
      queryKey[2] === "downloads")
  );
}

function shouldDehydrateAppQuery(query: Query): boolean {
  return query.state.status === "success" && !isFullDownloadsQueryKey(query.queryKey);
}

export function shouldRetryAppQuery(failureCount: number, error: unknown): boolean {
  const status =
    typeof error === "object" && error !== null
      ? (error as { status?: unknown }).status
      : undefined;

  if (typeof status === "number" && status >= 400 && status < 500) {
    return false;
  }

  return failureCount < 1;
}

export function createAppQueryClient(options?: QueryClientOptions) {
  return new QueryClient({
    defaultOptions: {
      queries: {
        enabled: options?.observerQueriesEnabled ?? true,
        gcTime: QUERY_CACHE_TIMES.inactive,
        refetchOnWindowFocus: false,
        retry: shouldRetryAppQuery,
        staleTime: QUERY_CACHE_TIMES.live,
      },
      dehydrate: {
        shouldDehydrateQuery: shouldDehydrateAppQuery,
      },
    },
  });
}

let browserQueryClient: QueryClient | null = null;

export function getBrowserAppQueryClient() {
  if (typeof window === "undefined") {
    throw new Error("The browser query client is not available during SSR.");
  }

  browserQueryClient ??= createAppQueryClient({ observerQueriesEnabled: true });
  return browserQueryClient;
}

export function createRouteQueryClient() {
  return typeof window === "undefined"
    ? createAppQueryClient({ observerQueriesEnabled: true })
    : getBrowserAppQueryClient();
}

export function visibleRefetchInterval(
  intervalMs: number,
  shouldRefetch: () => boolean = () => true,
): () => number | false {
  return () => {
    if (typeof document === "undefined") {
      return false;
    }

    return document.visibilityState === "visible" && shouldRefetch() ? intervalMs : false;
  };
}
