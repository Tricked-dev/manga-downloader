import { createAppQueryClient } from "$lib/query-client";
export { createHydrationState } from "$lib/query-hydration";

export function createServerQueryClient() {
  return createAppQueryClient({ observerQueriesEnabled: true });
}
