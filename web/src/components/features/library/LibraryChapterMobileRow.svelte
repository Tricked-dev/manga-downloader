<script lang="ts">
  import ChapterDownloadStatusCell from "$components/features/library/ChapterDownloadStatusCell.svelte";
  import { ACTIVE_DOWNLOAD_STATUSES, getTranslatedDownloadDisplayLabel } from "$lib/features/downloads/status";
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";
  import { Progress } from "$lib/ui/progress";
  import { t } from "$lib/i18n";
  import { formatChapterNumber, formatDateLabel } from "$lib/utils";
  import { Check } from "@lucide/svelte";

  let {
    chapterTableRow,
    mangaSource,
    row,
    chapterReadLabel,
    publicView = false,
    selectionBoxClass,
  } = $props<{
    chapterTableRow: any;
    mangaSource: string;
    row: any;
    chapterReadLabel: (chapter: any) => string;
    publicView?: boolean;
    selectionBoxClass: (selected: boolean) => string;
  }>();

  const chapter = $derived(chapterTableRow.chapter);
  const chapterDownload = $derived(chapterTableRow.download);
</script>

<article data-state={row.getIsSelected() ? "selected" : undefined} class="space-y-3 bg-card p-4 data-[state=selected]:bg-muted">
  <div class="flex items-start gap-3">
    {#if !publicView}
      <label class="group mt-0.5 inline-flex size-8 cursor-pointer items-center justify-center focus-within:ring-2 focus-within:ring-ring">
        <input
          type="checkbox"
          class="sr-only"
          checked={row.getIsSelected()}
          aria-label={$t("app.libraryDetail.selectChapter", { number: formatChapterNumber(chapter.chapter_number) })}
          onchange={row.getToggleSelectedHandler()}
        />
        <span class={selectionBoxClass(row.getIsSelected())}>
          {#if row.getIsSelected()}
            <Check class="size-3" />
          {/if}
        </span>
      </label>
    {/if}

    <div class="min-w-0 flex-1">
      <p class="text-sm font-medium">{$t("app.downloads.chapterWithNumber", { number: formatChapterNumber(chapter.chapter_number) })}</p>
      <p class="text-xs text-muted-foreground">
        {chapter.date_uploaded ? formatDateLabel(chapter.date_uploaded) : $t("app.manga.unknownUploadDate")}
      </p>
    </div>

    <Button
      href={`/read/${encodeURIComponent(mangaSource)}/${encodeURIComponent(chapter.source_id)}?chapterId=${encodeURIComponent(chapter.id)}&libraryId=${encodeURIComponent(chapter.manga_id)}`}
      variant="secondary"
      size="sm"
    >
      {$t("app.manga.read")}
    </Button>
  </div>

  <div class={`flex flex-wrap gap-2 ${publicView ? "" : "pl-11"}`}>
    <ChapterDownloadStatusCell download={chapterDownload} chapterDownloaded={chapter.downloaded} />
    <Badge variant={chapter.read_completed ? "success" : chapter.pages_read > 0 ? "secondary" : "outline"}>
      {chapterReadLabel(chapter)}
    </Badge>
  </div>

  <div class={publicView ? "" : "pl-11"}>
    {#if chapterDownload && ACTIVE_DOWNLOAD_STATUSES.has(chapterDownload.status)}
      <div class="space-y-1">
        <Progress value={Math.round(chapterDownload.progress)} class="h-2" />
        <p class="text-xs text-muted-foreground">
          {getTranslatedDownloadDisplayLabel(chapterDownload.status, $t)} {Math.round(chapterDownload.progress)}%
        </p>
      </div>
    {:else if chapterDownload}
      <p class="text-xs text-muted-foreground">{$t("app.downloads.progressComplete", { progress: Math.round(chapterDownload.progress) })}</p>
    {:else if chapter.downloaded}
      <p class="text-xs text-muted-foreground">{$t("app.downloads.progressComplete", { progress: 100 })}</p>
    {:else}
      <p class="text-xs text-muted-foreground">{$t("app.libraryDetail.noDownloadYet")}</p>
    {/if}
  </div>
</article>
