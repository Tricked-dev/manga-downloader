<script lang="ts">
  import { page } from "$app/state";
  import { Card, CardContent } from "$lib/ui/card";
  import { untrack } from "svelte";
  import LibraryDetailChaptersCard from "$components/features/library/LibraryDetailChaptersCard.svelte";
  import LibraryDetailHero from "$components/features/library/LibraryDetailHero.svelte";
  import LoadingState from "$components/common/LoadingState.svelte";
  import { isPublicShareViewActive } from "$lib/public-share";
  import { t } from "$lib/i18n";
  import { createLibraryDetailState } from "$lib/features/library/library-detail-state.svelte";
  import { useHydrate, useQueryClient, type DehydratedState } from "@tanstack/svelte-query";

  const { data } = $props<{
    data: {
      dehydratedState?: DehydratedState;
    };
  }>();
  const queryClient = useQueryClient();

  useHydrate(untrack(() => data.dehydratedState), undefined, queryClient);

  const publicView = $derived(isPublicShareViewActive(page.data.auth, page.url));
  const state = createLibraryDetailState(() => $t, {
    dehydratedState: untrack(() => data.dehydratedState),
    isPublicView: () => publicView,
  });
</script>

<svelte:head>
  <title>{state.manga?.title ?? $t("app.library.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<div class="space-y-8">
  {#if state.pageLoading}
    <LoadingState label={$t("app.libraryDetail.loadingLibraryManga")} />
  {:else if state.pageError}
    <Card class="border-destructive/30 bg-destructive/5">
      <CardContent class="p-4">
        <p class="text-sm font-medium text-destructive">{state.pageError}</p>
      </CardContent>
    </Card>
  {:else if state.manga}
    <LibraryDetailHero
      availableCategories={state.availableCategories}
      categorySaveMessage={state.categorySaveMessage}
      comicInfoRows={state.comicInfoRows}
      effectiveSelectedCategory={state.effectiveSelectedCategory}
      manga={state.manga}
      publicShareAuth={page.data.auth}
      {publicView}
      removeMessage={state.removeMessage}
      removeMessageIsError={state.removeMessageIsError}
      removePending={state.removeFromLibraryMutation.isPending}
      savingCategory={state.savingCategory}
      settingsPending={state.settingsQuery.isPending}
      onRemove={state.removeFromLibrary}
      onSaveCategory={state.saveCategory}
      onSelectCategory={state.updateSelectedCategory}
    />

    <LibraryDetailChaptersCard
      chapterActionMessage={state.chapterActionMessage}
      chapterActionMessageIsError={state.chapterActionMessageIsError}
      chapterColumns={state.chapterColumns}
      chapterHeaderCellClass={state.chapterHeaderCellClass}
      chapterReadLabel={state.chapterReadLabel}
      chapterTable={state.chapterTable}
      chaptersError={state.chaptersError}
      chaptersLoading={state.chaptersLoading}
      clearActionDisabled={state.clearActionDisabled}
      comicInfoRefreshMessage={state.comicInfoRefreshMessage}
      comicInfoRefreshMessageIsError={state.comicInfoRefreshMessageIsError}
      deleteActionDisabled={state.deleteActionDisabled}
      disabledActionClass={state.disabledActionClass}
      downloadActionDisabled={state.downloadActionDisabled}
      mangaSource={state.manga.source}
      markReadActionDisabled={state.markReadActionDisabled}
      markUnreadActionDisabled={state.markUnreadActionDisabled}
      reencodeActionDisabled={state.reencodeActionDisabled}
      refreshChaptersPending={state.refreshChaptersMutation.isPending}
      refreshComicInfoPending={state.refreshComicInfoMutation.isPending}
      refreshMessage={state.refreshMessage}
      refreshMessageIsError={state.refreshMessageIsError}
      selectedCount={state.selectedCount}
      selectedDeleteLoading={state.selectedDeleteLoading}
      selectedReadLoading={state.selectedReadLoading}
      selectedReencodeLoading={state.selectedReencodeLoading}
      selectedUnreadLoading={state.selectedUnreadLoading}
      bulkDownloadLoading={state.bulkDownloadLoading}
      {publicView}
      selectionBoxClass={state.selectionBoxClass}
      sortedChapterRows={state.sortedChapterRows}
      sortLabel={state.sortLabel}
      onClearSelection={state.handleClearSelection}
      onDeleteSelected={state.handleDeleteSelected}
      onDownloadSelected={state.handleDownloadSelected}
      onMarkSelectedRead={state.handleMarkSelectedRead}
      onMarkSelectedUnread={state.handleMarkSelectedUnread}
      onReencodeSelected={state.handleReencodeSelected}
      onRefreshChapters={state.refreshChapters}
      onRefreshComicInfo={state.refreshComicInfo}
    />
  {/if}
</div>
