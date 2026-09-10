import {
  getGetDownloadsQueryKey,
  getGetLibraryMangaChaptersQueryKey,
  getGetLibraryMangaQueryKey,
  getGetLibraryQueryKey,
} from "@manga-server/api-client/generated";
import { orderChaptersForChronologicalDownload } from "$lib/features/library/library-chapter-actions";
import type { DownloadItem, LibraryChapter } from "$lib/types";

interface Mutation<TVariables, TResult = unknown> {
  mutateAsync(variables: TVariables): Promise<TResult>;
}

interface QueryClientLike {
  invalidateQueries(options: { queryKey: readonly unknown[] }): Promise<unknown>;
  removeQueries(options: { queryKey: readonly unknown[] }): unknown;
}

interface EnqueueDownloadsResult {
  enqueued: number;
}

export async function invalidateLocalLibraryDownloads(queryClient: QueryClientLike) {
  await queryClient.invalidateQueries({ queryKey: getGetDownloadsQueryKey() });
}

export async function invalidateLocalLibraryState(queryClient: QueryClientLike, libraryId: string) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: getGetLibraryQueryKey() }),
    queryClient.invalidateQueries({ queryKey: getGetLibraryMangaQueryKey(libraryId) }),
    queryClient.invalidateQueries({ queryKey: getGetLibraryMangaChaptersQueryKey(libraryId) }),
  ]);
}

export async function queueLocalLibraryChaptersForDownload(
  mutation: Mutation<{ data: { chapter_ids: string[]; manga_id: string } }, EnqueueDownloadsResult>,
  libraryId: string,
  chapters: readonly LibraryChapter[],
): Promise<number> {
  const ordered = orderChaptersForChronologicalDownload(chapters);
  const result = await mutation.mutateAsync({
    data: {
      chapter_ids: ordered.map((chapter) => chapter.id),
      manga_id: libraryId,
    },
  });
  return result.enqueued;
}

export async function deleteLocalLibraryDownloads(
  mutation: Mutation<{ id: string }>,
  downloads: readonly DownloadItem[],
) {
  for (const download of downloads) {
    await mutation.mutateAsync({ id: download.id });
  }
}

export async function reencodeLocalLibraryDownloads(
  mutation: Mutation<{ id: string }>,
  downloads: readonly DownloadItem[],
) {
  for (const download of downloads) {
    await mutation.mutateAsync({ id: download.id });
  }
}

export async function markLocalLibraryChaptersRead(
  mutation: Mutation<{ chapterId: string; data: { completed: boolean; page: number } }>,
  chapters: readonly LibraryChapter[],
) {
  for (const chapter of chapters) {
    await mutation.mutateAsync({
      chapterId: chapter.id,
      data: {
        completed: true,
        page: chapter.pages_read,
      },
    });
  }
}

export async function markLocalLibraryChaptersUnread(
  mutation: Mutation<{ chapterId: string }>,
  chapters: readonly LibraryChapter[],
) {
  for (const chapter of chapters) {
    await mutation.mutateAsync({ chapterId: chapter.id });
  }
}

export async function saveLocalLibraryCategory(
  mutation: Mutation<{ data: { category: string }; id: string }>,
  queryClient: QueryClientLike,
  libraryId: string,
  category: string,
) {
  await mutation.mutateAsync({
    data: { category },
    id: libraryId,
  });
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: getGetLibraryQueryKey() }),
    queryClient.invalidateQueries({ queryKey: getGetLibraryMangaQueryKey(libraryId) }),
  ]);
}

export async function refreshLocalLibraryChapters(
  mutation: Mutation<{ id: string }>,
  libraryId: string,
) {
  await mutation.mutateAsync({ id: libraryId });
}

export async function refreshLocalLibraryComicInfo(
  mutation: Mutation<{ id: string }, { files_rewritten: number }>,
  queryClient: QueryClientLike,
  libraryId: string,
): Promise<number> {
  const result = await mutation.mutateAsync({ id: libraryId });
  await invalidateLocalLibraryState(queryClient, libraryId);
  return result.files_rewritten;
}

export async function removeLocalLibraryManga(
  mutation: Mutation<{ id: string }>,
  queryClient: QueryClientLike,
  libraryId: string,
) {
  await mutation.mutateAsync({ id: libraryId });
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: getGetLibraryQueryKey() }),
    queryClient.removeQueries({ queryKey: getGetLibraryMangaQueryKey(libraryId) }),
    queryClient.removeQueries({ queryKey: getGetLibraryMangaChaptersQueryKey(libraryId) }),
  ]);
}
