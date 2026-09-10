import { error } from "@sveltejs/kit";
import {
  applyBackendAuthorization,
  getBackendBaseUrl,
  getBackendFetch,
} from "$lib/server/api/backend-config";
import type { RequestEvent } from "@sveltejs/kit";

interface ChapterListResponse {
  items?: Array<{ id?: unknown }>;
}

export async function assertPublicSharedLibraryChapterAccess(
  event: RequestEvent,
  chapterId: string,
): Promise<void> {
  if (!event.locals.publicShare) {
    return;
  }

  const sharedLibraryId = event.url.searchParams.get("libraryId");
  const shareSegments = event.locals.publicShare.share.pathname.split("/").filter(Boolean);
  if (!sharedLibraryId || shareSegments[0] !== "library" || shareSegments[1] !== sharedLibraryId) {
    throw error(404, "Chapter not found for public share");
  }

  const chaptersUrl = new URL(
    `${getBackendBaseUrl(event.platform)}/v1/library/${encodeURIComponent(sharedLibraryId)}/chapters`,
  );
  const headers = applyBackendAuthorization(new Headers(), event.platform);
  const response = await getBackendFetch(event.platform)(chaptersUrl, { headers });
  if (!response.ok) {
    throw error(response.status, "Unable to verify public chapter access");
  }

  const payload = (await response.json().catch(() => null)) as ChapterListResponse | null;
  const chapterAllowed = payload?.items?.some((chapter) => chapter.id === chapterId) ?? false;
  if (!chapterAllowed) {
    throw error(404, "Chapter not found for public share");
  }
}
