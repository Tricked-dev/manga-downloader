import type { LibraryChapter, LibraryManga } from "$lib/types";

export interface UpdatesSection {
  label: string;
  items: UpdateFeedItem[];
}

export interface UpdateFeedItem {
  chapter: LibraryChapter;
  pickedUpAt: Date | null;
  pickedUpTimeLabel: string;
  releaseAt: Date | null;
  releaseDateLabel: string;
  releaseTimeLabel: string;
  manga: LibraryManga;
}
