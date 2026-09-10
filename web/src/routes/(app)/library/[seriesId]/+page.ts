import { createAppApiContext, customFetchOptions } from "@manga-server/api-client";
import { getGetLibraryMangaQueryOptions, getGetLibraryMangaChaptersQueryOptions } from "@manga-server/api-client/generated";
import { getBrowserAppQueryClient } from "$lib/query-client";
import type { PageLoad } from "./$types";

export const load: PageLoad = async ({ fetch, parent, params }) => {
  await parent();
  const client = getBrowserAppQueryClient();
  const request = customFetchOptions(createAppApiContext(fetch));
  await Promise.all([
    client.ensureQueryData(getGetLibraryMangaQueryOptions(params.seriesId, { request })),
    client.ensureQueryData(getGetLibraryMangaChaptersQueryOptions(params.seriesId, { request })),
  ]);
  return {};
};
