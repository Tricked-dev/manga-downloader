import { browser } from "$app/environment";
import { createAppApiContext, customFetchOptions } from "@manga-server/api-client";
import { getListSourcesQueryOptions } from "@manga-server/api-client/generated";
import { QUERY_CACHE_TIMES, createRouteQueryClient } from "$lib/query-client";
import { createHydrationState } from "$lib/query-hydration";
import type { LayoutLoad } from "./$types";

export const load: LayoutLoad = async ({ fetch, parent }) => {
  await parent();
  const queryClient = createRouteQueryClient();
  const context = createAppApiContext(fetch);
  const request = customFetchOptions(context);
  const sources = await queryClient.ensureQueryData(
    getListSourcesQueryOptions({
      query: { staleTime: QUERY_CACHE_TIMES.stable },
      request,
    }),
  );

  return {
    ...(browser ? {} : createHydrationState(queryClient)),
    sources,
  };
};
