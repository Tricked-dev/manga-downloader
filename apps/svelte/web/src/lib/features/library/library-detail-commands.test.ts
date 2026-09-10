import { describe, expect, it } from "vitest";

import type { LibraryChapter } from "$lib/types";
import {
  queueLocalLibraryChaptersForDownload,
  removeLocalLibraryManga,
} from "./library-detail-commands";

function chapter(overrides: Partial<LibraryChapter> = {}): LibraryChapter {
  return {
    chapter_number: 1,
    date_uploaded: "",
    downloaded: false,
    fetched_at: "",
    id: "chapter-1",
    is_new: false,
    last_read_at: null,
    manga_id: "series-1",
    pages_read: 0,
    read_completed: false,
    source_id: "remote-chapter-1",
    title: "",
    ...overrides,
  };
}

describe("Local Library detail commands", () => {
  it("queues selected chapters in chronological order", async () => {
    const payloads: Array<{ data: { chapter_ids: string[]; manga_id: string } }> = [];
    const mutation = {
      async mutateAsync(payload: { data: { chapter_ids: string[]; manga_id: string } }) {
        payloads.push(payload);
        return { enqueued: payload.data.chapter_ids.length };
      },
    };

    const enqueued = await queueLocalLibraryChaptersForDownload(mutation, "series-1", [
      chapter({ chapter_number: 3, id: "chapter-3" }),
      chapter({ chapter_number: 1, id: "chapter-1" }),
      chapter({ chapter_number: 2, id: "chapter-2" }),
    ]);

    expect(enqueued).toBe(3);
    expect(payloads).toEqual([
      {
        data: {
          chapter_ids: ["chapter-1", "chapter-2", "chapter-3"],
          manga_id: "series-1",
        },
      },
    ]);
  });

  it("removes cached Local Library queries after removing a series", async () => {
    const invalidated: unknown[][] = [];
    const removed: unknown[][] = [];
    const mutation = {
      async mutateAsync(payload: { id: string }) {
        expect(payload).toEqual({ id: "series-1" });
      },
    };
    const queryClient = {
      async invalidateQueries(options: { queryKey: readonly unknown[] }) {
        invalidated.push([...options.queryKey]);
      },
      removeQueries(options: { queryKey: readonly unknown[] }) {
        removed.push([...options.queryKey]);
      },
    };

    await removeLocalLibraryManga(mutation, queryClient, "series-1");

    expect(invalidated).toHaveLength(1);
    expect(removed).toHaveLength(2);
  });
});
