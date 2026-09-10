import type { SvelteComponent, Component, ComponentType } from "svelte";

export type IconComponent = Component | ComponentType | (new (...args: any[]) => SvelteComponent);
export type LibraryManga = import("@manga-server/api-client/models").LibraryMangaData;
export type Source = import("@manga-server/api-client/models").SourceData;
export type SourceSettings = import("@manga-server/api-client/models").SourceSettingsData;
export type SearchResult = import("@manga-server/api-client/models").SearchResultData;
export type DownloadItem = import("@manga-server/api-client/models").DownloadItemData;
export type LibraryChapter = import("@manga-server/api-client/models").LibraryChapterData;
