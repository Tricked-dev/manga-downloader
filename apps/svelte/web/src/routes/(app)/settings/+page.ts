import { browser } from "$app/environment";
import { createAppApiContext, customFetchOptions } from "@manga-server/api-client";
import {
  getGetArchiveIndexStatusQueryOptions,
  getGetSettingsQueryOptions,
  getListSourcesQueryOptions,
} from "@manga-server/api-client/generated";
import { QUERY_CACHE_TIMES, createRouteQueryClient } from "$lib/query-client";
import { createHydrationState } from "$lib/query-hydration";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch }) => {
  const queryClient = createRouteQueryClient();
  const context = createAppApiContext(fetch);
  const request = customFetchOptions(context);

  await Promise.all([
    queryClient.ensureQueryData(
      getGetSettingsQueryOptions({
        query: { staleTime: QUERY_CACHE_TIMES.stable },
        request,
      }),
    ),
    queryClient.ensureQueryData(
      getListSourcesQueryOptions({
        query: { staleTime: QUERY_CACHE_TIMES.stable },
        request,
      }),
    ),
    queryClient.ensureQueryData(
      getGetArchiveIndexStatusQueryOptions({
        query: { staleTime: QUERY_CACHE_TIMES.live },
        request,
      }),
    ),
  ]);

  return browser ? {} : createHydrationState(queryClient);
};
