import type { DownloadItem, LibraryManga } from "$lib/types";

export type LibraryView = {
  categoryOptions: string[];
  downloadsByManga: Record<string, DownloadItem[]>;
  genreOptions: string[];
  languageOptions: string[];
  statusOptions: string[];
  ageRatingOptions: string[];
  filteredLibrary: LibraryManga[];
};

export type LibraryMetadataFilters = {
  ageRating?: string;
  genre?: string;
  language?: string;
  search?: string;
  status?: string;
};

export function getLibraryView(
  library: readonly LibraryManga[],
  downloads: readonly DownloadItem[],
  selectedCategory: string,
  filters: LibraryMetadataFilters = {},
): LibraryView {
  const ageRatingsByName: Record<string, true> = {};
  const categoriesByName: Record<string, true> = {};
  const downloadsByManga: Record<string, DownloadItem[]> = {};
  const genresByName: Record<string, true> = {};
  const languagesByName: Record<string, true> = {};
  const statusesByName: Record<string, true> = {};
  const normalizedSearch = normalizeSearch(filters.search);
  const selectedAgeRating = normalizeFilter(filters.ageRating);
  const selectedGenre = normalizeFilter(filters.genre);
  const selectedLanguage = normalizeFilter(filters.language);
  const selectedStatus = normalizeFilter(filters.status);

  for (const download of downloads) {
    const mangaDownloads = downloadsByManga[download.manga_id];
    if (mangaDownloads) {
      mangaDownloads.push(download);
    } else {
      downloadsByManga[download.manga_id] = [download];
    }
  }

  const filteredLibrary: LibraryManga[] = [];
  for (const manga of library) {
    const category = (manga.category || "").trim();
    if (category.length > 0) {
      categoriesByName[category] = true;
    }
    collectValues(statusesByName, [manga.status]);
    collectValues(genresByName, [...manga.genres, manga.comic_info?.genre]);
    collectValues(languagesByName, [manga.language, manga.comic_info?.language_iso]);
    collectValues(ageRatingsByName, [manga.comic_info?.age_rating]);

    if (
      (selectedCategory === "all" || category === selectedCategory) &&
      matchesSearch(manga, normalizedSearch) &&
      matchesValue(manga.status, selectedStatus) &&
      matchesList([...manga.genres, manga.comic_info?.genre], selectedGenre) &&
      matchesList([manga.language, manga.comic_info?.language_iso], selectedLanguage) &&
      matchesList([manga.comic_info?.age_rating], selectedAgeRating)
    ) {
      filteredLibrary.push(manga);
    }
  }

  return {
    categoryOptions: [
      "all",
      ...Object.keys(categoriesByName).toSorted((left, right) => left.localeCompare(right)),
    ],
    downloadsByManga,
    genreOptions: sortedOptions(genresByName),
    languageOptions: sortedOptions(languagesByName),
    statusOptions: sortedOptions(statusesByName),
    ageRatingOptions: sortedOptions(ageRatingsByName),
    filteredLibrary,
  };
}

function collectValues(target: Record<string, true>, values: Array<string | null | undefined>) {
  for (const value of values) {
    for (const item of splitMetadataValue(value)) {
      target[item] = true;
    }
  }
}

function sortedOptions(options: Record<string, true>): string[] {
  return ["all", ...Object.keys(options).toSorted((left, right) => left.localeCompare(right))];
}

function splitMetadataValue(value: string | null | undefined): string[] {
  return (value ?? "")
    .split(",")
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

function normalizeFilter(value: string | null | undefined): string {
  const trimmed = (value ?? "").trim();
  return trimmed === "all" ? "" : trimmed.toLocaleLowerCase();
}

function normalizeSearch(value: string | null | undefined): string {
  return (value ?? "").trim().toLocaleLowerCase();
}

function matchesValue(value: string | null | undefined, selected: string): boolean {
  if (!selected) {
    return true;
  }

  return (value ?? "").trim().toLocaleLowerCase() === selected;
}

function matchesList(values: Array<string | null | undefined>, selected: string): boolean {
  if (!selected) {
    return true;
  }

  return values.some((value) =>
    splitMetadataValue(value).some((item) => item.toLocaleLowerCase() === selected),
  );
}

function matchesSearch(manga: LibraryManga, query: string): boolean {
  if (!query) {
    return true;
  }

  return [
    manga.title,
    manga.source,
    manga.source_id,
    manga.category,
    manga.description,
    manga.author,
    manga.status,
    manga.language,
    ...manga.genres,
    manga.comic_info?.title,
    manga.comic_info?.series,
    manga.comic_info?.summary,
    manga.comic_info?.writer,
    manga.comic_info?.genre,
    manga.comic_info?.age_rating,
    manga.comic_info?.language_iso,
    manga.comic_info?.web,
  ].some((value) => (value ?? "").toLocaleLowerCase().includes(query));
}
