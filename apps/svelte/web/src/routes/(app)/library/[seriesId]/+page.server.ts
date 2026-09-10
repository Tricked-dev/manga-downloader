import { createAppApiContext, customFetchOptions } from "@manga-server/api-client";
import {
  getGetLibraryMangaChaptersQueryOptions,
  getGetLibraryMangaQueryOptions,
  getGetSettingsQueryKey,
  getGetSettingsQueryOptions,
} from "@manga-server/api-client/generated";
import { publicShareRequestHeaders } from "$lib/server/auth/public-share";
import { createHydrationState, createServerQueryClient } from "$lib/server/query";
import type { PageServerLoad } from "./$types";

export const load: PageServerLoad = async ({ fetch, locals, params, request: eventRequest }) => {
  const queryClient = createServerQueryClient();
  const context = createAppApiContext(fetch);
  const libraryId = params.seriesId;
  const publicShare = locals.session ? null : locals.publicShare;
  const request = {
    ...customFetchOptions(context),
    headers: publicShareRequestHeaders(publicShare, eventRequest.headers.get("cookie")),
  };
  const pageQueries: Array<Promise<unknown>> = [
    queryClient.ensureQueryData(getGetLibraryMangaQueryOptions(libraryId, { request })),
  ];

  if (!publicShare) {
    pageQueries.push(
      locals.backendSettingsFresh && locals.backendSettings
        ? Promise.resolve(
            queryClient.setQueryData(getGetSettingsQueryKey(), locals.backendSettings),
          )
        : queryClient.ensureQueryData(getGetSettingsQueryOptions({ request })),
    );
  }

  await Promise.all(pageQueries);

  await queryClient
    .ensureQueryData(getGetLibraryMangaChaptersQueryOptions(libraryId, { request }))
    .catch(() => undefined);

  return createHydrationState(queryClient);
};
