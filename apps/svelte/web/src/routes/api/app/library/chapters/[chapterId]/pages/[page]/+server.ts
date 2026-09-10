import { proxyApiRequest } from "$lib/server/api/backend-client";
import { assertPublicSharedLibraryChapterAccess } from "$lib/server/api/public-share-chapters";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async (event) => {
  await assertPublicSharedLibraryChapterAccess(event, event.params.chapterId);
  return proxyApiRequest(
    event,
    `/v1/library/chapters/${encodeURIComponent(event.params.chapterId)}/pages/${encodeURIComponent(event.params.page)}`,
  );
};
