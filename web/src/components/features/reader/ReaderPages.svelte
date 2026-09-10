<script lang="ts">
  import { Alert, AlertDescription, AlertTitle } from "$lib/ui/alert";
  import { Button } from "$lib/ui/button";
  import { Card } from "$lib/ui/card";
  import LoadingState from "$components/common/LoadingState.svelte";
  import ReaderPageImage from "$components/features/reader/ReaderPageImage.svelte";
  import { t } from "$lib/i18n";
  import { ArrowLeft, ArrowRight } from "@lucide/svelte";

  let {
    currentPageIndex,
    loading,
    mode,
    onImageError,
    onNextPage,
    onPageLoad,
    onPreviousPage,
    onRetryLoad,
    pageImageUrl,
    pageLoadError,
    pages,
    scrollPageHeight,
    shouldRenderScrollPage,
  } = $props<{
    currentPageIndex: number;
    loading: boolean;
    mode: "scroll" | "page";
    onImageError: (pageUrl: string) => void;
    onNextPage: () => void;
    onPageLoad: (index: number, event: Event) => void;
    onPreviousPage: () => void;
    onRetryLoad: () => void;
    pageImageUrl: (pageUrl: string) => string;
    pageLoadError: string;
    pages: string[];
    scrollPageHeight: (index: number) => string;
    shouldRenderScrollPage: (index: number) => boolean;
  }>();
</script>

{#if loading}
  <LoadingState label={$t("app.reader.loadingPages")} />
{:else if pageLoadError}
  <Alert variant="destructive">
    <AlertTitle>{$t("app.reader.unavailable")}</AlertTitle>
    <AlertDescription>
      <p>{pageLoadError}</p>
      <Button variant="outline" size="sm" class="mt-3" onclick={onRetryLoad}>
        {$t("app.reader.retry")}
      </Button>
    </AlertDescription>
  </Alert>
{:else if pages.length === 0}
  <Card class="border-dashed p-8 text-center">
    <p class="text-sm font-medium">{$t("app.reader.noPagesTitle")}</p>
    <p class="mt-2 text-sm text-muted-foreground">
      {$t("app.reader.noPagesDescription")}
    </p>
  </Card>
{:else if mode === "scroll"}
  <div class="mx-auto flex max-w-3xl flex-col items-center gap-1">
    {#each pages as pageUrl, i (`${i}:${pageUrl}`)}
      <div data-reader-page-index={i} class="w-full">
        <ReaderPageImage
          pageIndex={i}
          {pageUrl}
          altPage={i + 1}
          imageUrl={pageImageUrl(pageUrl)}
          fetchPriority={i === 0 ? "high" : "low"}
          loading={i === 0 ? "eager" : "lazy"}
          onImageError={onImageError}
          onPageLoad={onPageLoad}
          placeholderHeight={scrollPageHeight(i)}
          render={shouldRenderScrollPage(i)}
        />
      </div>
    {/each}
  </div>
{:else}
  <div class="mx-auto flex max-w-3xl flex-col items-center">
    {#if pages[currentPageIndex]}
      <ReaderPageImage
        pageIndex={currentPageIndex}
        pageUrl={pages[currentPageIndex]}
        altPage={currentPageIndex + 1}
        imageUrl={pageImageUrl(pages[currentPageIndex])}
        class="w-full cursor-pointer"
        fetchPriority="high"
        loading="eager"
        onAdvance={onNextPage}
        onImageError={onImageError}
      />
    {/if}
    <div class="mt-4 flex items-center gap-4 py-6">
      <Button
        variant="secondary"
        onclick={onPreviousPage}
        disabled={currentPageIndex === 0}
      >
        <ArrowLeft class="w-4 h-4 mr-1" /> {$t("app.reader.previous")}
      </Button>
      <span class="text-sm font-medium text-muted-foreground">{currentPageIndex + 1} / {pages.length}</span>
      <Button
        variant="secondary"
        onclick={onNextPage}
        disabled={currentPageIndex === pages.length - 1}
      >
        {$t("app.reader.next")} <ArrowRight class="w-4 h-4 ml-1" />
      </Button>
    </div>
  </div>
{/if}
