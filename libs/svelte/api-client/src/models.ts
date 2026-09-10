export type {
  ChapterResponse as SourceChapterData,
  ChapterRow as LibraryChapterData,
  CreatedResourceResponse,
  DownloadEnqueueResponse,
  DownloadRow as DownloadItemData,
  LibraryMangaResponse as LibraryMangaData,
  LibraryUpdateResponse,
  MangaResponse as MangaSummaryData,
  OperationStatusResponse,
  PluginArtifactResponse as PluginArtifactData,
  ReencodeDownloadResponse,
  RefreshLibraryMetadataResponse,
  SearchResponse as SearchResultData,
  SourceInfo as SourceData,
  SourceSettingsResponse as SourceSettingsData,
  UploadPluginResponse,
} from "./generated/model";

export type ReencodeLibraryResponseData = {
  files_processed: number;
  images_reencoded: number;
};
