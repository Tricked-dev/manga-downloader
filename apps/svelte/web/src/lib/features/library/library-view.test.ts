import { describe, expect, it } from "vitest";

import type { DownloadItem, LibraryManga } from "$lib/types";
import { getLibraryView } from "./library-view";

function manga(
  overrides: Partial<LibraryManga> & Pick<LibraryManga, "id" | "title">,
): LibraryManga {
  return {
    author: "Author",
    auto_download: null,
    category: "",
    comic_info: {
      age_rating: "Rating Pending",
      genre: null,
      language_iso: "en",
      series: overrides.title,
      summary: "Description",
      title: overrides.title,
      web: null,
      writer: "Author",
    },
    cover_proxy_url: null,
    cover_url: "https://manga.example/cover.jpg",
    description: "Description",
    downloaded_chapters: 0,
    genres: [],
    is_nsfw: false,
    language: "en",
    last_updated: "2026-05-21T00:00:00Z",
    source: "example",
    source_base_url: "https://manga.example",
    source_id: `${overrides.id}-remote`,
    status: "ongoing",
    total_chapters: 0,
    ...overrides,
  };
}

function download(overrides: Pick<DownloadItem, "id" | "manga_id" | "status">): DownloadItem {
  return {
    chapter_id: `${overrides.id}-chapter`,
    chapter_number: 1,
    chapter_source_id: `${overrides.id}-remote-chapter`,
    chapter_title: "Chapter 1",
    error: null,
    manga_source: "Example",
    manga_source_id: `${overrides.manga_id}-remote`,
    manga_title: "Series",
    progress: 0,
    ...overrides,
  };
}

describe("getLibraryView", () => {
  it("builds sorted category options while ignoring blank categories", () => {
    const view = getLibraryView(
      [
        manga({ category: " Reading ", id: "one", title: "One" }),
        manga({ category: "", id: "two", title: "Two" }),
        manga({ category: "Complete", id: "three", title: "Three" }),
        manga({ category: "Reading", id: "four", title: "Four" }),
      ],
      [],
      "all",
    );

    expect(view.categoryOptions).toEqual(["all", "Complete", "Reading"]);
    expect(view.filteredLibrary.map((item) => item.id)).toEqual(["one", "two", "three", "four"]);
  });

  it("filters the library by the trimmed category value", () => {
    const view = getLibraryView(
      [
        manga({ category: " Reading ", id: "one", title: "One" }),
        manga({ category: "Complete", id: "two", title: "Two" }),
        manga({ category: "Reading", id: "three", title: "Three" }),
      ],
      [],
      "Reading",
    );

    expect(view.filteredLibrary.map((item) => item.id)).toEqual(["one", "three"]);
  });

  it("groups active downloads by library manga id", () => {
    const view = getLibraryView(
      [manga({ id: "series-1", title: "One" }), manga({ id: "series-2", title: "Two" })],
      [
        download({ id: "download-1", manga_id: "series-1", status: "queued" }),
        download({ id: "download-2", manga_id: "series-1", status: "fetch" }),
        download({ id: "download-3", manga_id: "series-2", status: "completed" }),
      ],
      "all",
    );

    expect(view.downloadsByManga["series-1"].map((item) => item.id)).toEqual([
      "download-1",
      "download-2",
    ]);
    expect(view.downloadsByManga["series-2"].map((item) => item.id)).toEqual(["download-3"]);
  });

  it("filters by ComicInfo metadata", () => {
    const view = getLibraryView(
      [
        manga({
          comic_info: {
            age_rating: "Adults Only 18+",
            genre: "Action, Horror",
            language_iso: "ja",
            series: "Midnight Archive",
            summary: "A library of dangerous stories",
            title: "Volume 1",
            web: "https://manga.example/midnight",
            writer: "A. Writer",
          },
          id: "one",
          title: "Midnight",
        }),
        manga({
          comic_info: {
            age_rating: "Rating Pending",
            genre: "Comedy",
            language_iso: "en",
            series: "Lunch Break",
            summary: "Office comedy",
            title: "Volume 1",
            web: null,
            writer: "B. Writer",
          },
          id: "two",
          title: "Lunch",
        }),
      ],
      [],
      "all",
      {
        ageRating: "Adults Only 18+",
        genre: "Horror",
        language: "ja",
        search: "dangerous",
      },
    );

    expect(view.filteredLibrary.map((item) => item.id)).toEqual(["one"]);
    expect(view.genreOptions).toEqual(["all", "Action", "Comedy", "Horror"]);
    expect(view.languageOptions).toEqual(["all", "en", "ja"]);
    expect(view.ageRatingOptions).toEqual(["all", "Adults Only 18+", "Rating Pending"]);
  });
});
