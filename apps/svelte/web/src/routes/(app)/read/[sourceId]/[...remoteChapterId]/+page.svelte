<script lang="ts">
  import { page } from "$app/state";
  import ReaderImageFallbackCard from "$components/features/reader/ReaderImageFallbackCard.svelte";
  import ReaderPages from "$components/features/reader/ReaderPages.svelte";
  import ReaderToolbar from "$components/features/reader/ReaderToolbar.svelte";
  import { createReaderSession } from "$lib/features/reader/reader-session.svelte";
  import { t } from "$lib/i18n";

  const source = $derived(page.params.sourceId);
  const chapterId = $derived(page.params.remoteChapterId);
  const sourceName = $derived(source ?? "");
  const chapterSourceId = $derived(chapterId ?? "");
  const libraryChapterId = $derived(page.url.searchParams.get("chapterId") ?? "");
  const libraryMangaId = $derived(page.url.searchParams.get("libraryId") ?? "");

  const session = createReaderSession({
    getChapterSourceId: () => chapterSourceId,
    getLibraryChapterId: () => libraryChapterId,
    getLibraryMangaId: () => libraryMangaId,
    getSourceName: () => sourceName,
    translate: (key, vars) => $t(key, vars),
  });
</script>

<svelte:head>
  <title>{$t("app.reader.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<svelte:window
  onkeydown={session.handleKeydown}
  onresize={session.scheduleActiveScrollPageUpdate}
  onscroll={session.scheduleActiveScrollPageUpdate}
/>

<div class="space-y-6">
  <ReaderToolbar
    mode={session.mode}
    pageCountLabel={session.pageCountLabel}
    onBack={session.goBack}
    onPageMode={session.setPageMode}
    onScrollMode={session.setScrollMode}
  />

  {#if session.imageLoadFailed}
    <ReaderImageFallbackCard
      conflict={session.imageLoadConflict}
      firstFailedUrl={session.firstFailedUrl}
      sourceBaseUrl={session.sourceBaseUrl}
      onRetryProxy={session.retryProxyImages}
    />
  {/if}

  <ReaderPages
    currentPageIndex={session.currentPageIndex}
    loading={session.loading}
    mode={session.mode}
    pageImageUrl={session.pageImageUrl}
    pageLoadError={session.pageLoadError}
    pages={session.pages}
    scrollPageHeight={session.scrollPageHeight}
    shouldRenderScrollPage={session.shouldRenderScrollPage}
    onImageError={session.handleImageError}
    onNextPage={session.goToNextPage}
    onPageLoad={session.recordScrollPageHeight}
    onPreviousPage={session.goToPreviousPage}
    onRetryLoad={session.retryLoadPages}
  />
</div>
