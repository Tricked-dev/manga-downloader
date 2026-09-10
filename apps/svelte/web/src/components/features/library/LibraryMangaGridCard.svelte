<script lang="ts">
  import { Badge } from "$lib/ui/badge";
  import DownloadSummaryBadges from "$components/downloads/DownloadSummaryBadges.svelte";
  import MangaCard from "$components/manga/MangaCard.svelte";
  import { t } from "$lib/i18n";
  import type { DownloadItem, LibraryManga } from "$lib/types";
  import { formatDateLabel } from "$lib/utils";
  import { CircleAlert } from "@lucide/svelte";

  let {
    manga,
    downloads,
  } = $props<{
    manga: LibraryManga;
    downloads: DownloadItem[];
  }>();

  const undownloadedCount = $derived(getUndownloadedCount(manga));
  const writer = $derived(displayWriter(manga));

  function getUndownloadedCount(manga: LibraryManga) {
    const total = manga.total_chapters ?? 0;
    const downloaded = manga.downloaded_chapters ?? 0;
    return Math.max(total - downloaded, 0);
  }

  function displayWriter(manga: LibraryManga) {
    const comicWriter = manga.comic_info?.writer?.trim();
    const author = manga.author?.trim();

    return comicWriter && comicWriter !== author ? comicWriter : "";
  }
</script>

<MangaCard
  {manga}
  href={`/library/${manga.id}`}
  coverFormat="avif"
  fetchPriority="low"
  imageLoading="lazy"
>
  {#snippet details()}
    <div class="space-y-2">
      <p class="text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
        {$t("app.library.lastUpdated", { date: formatDateLabel(manga.last_updated, $t("app.library.unknown")) })}
      </p>
      <p class="text-xs text-muted-foreground/90">
        {$t("app.library.source", { source: manga.source })}
      </p>
      <div class="flex flex-wrap gap-1.5 empty:hidden">
        {#if writer}
          <Badge variant="secondary" class="text-[10px]">{$t("app.library.writer", { writer })}</Badge>
        {/if}
        {#if manga.comic_info?.language_iso}
          <Badge variant="outline" class="text-[10px]">{manga.comic_info.language_iso}</Badge>
        {/if}
        {#if manga.comic_info?.age_rating}
          <Badge variant={manga.is_nsfw ? "destructive" : "outline"} class="text-[10px]">
            {manga.comic_info.age_rating}
          </Badge>
        {/if}
      </div>

      {#if undownloadedCount > 0}
        <div class="flex flex-wrap gap-1.5">
          <Badge variant="warning" class="gap-1 text-[10px]">
            <CircleAlert class="h-3 w-3" />
            {$t("app.library.undownloaded", { count: undownloadedCount })}
          </Badge>
        </div>
      {/if}

      <DownloadSummaryBadges items={downloads} showCompleted={false} />
    </div>
  {/snippet}
</MangaCard>
