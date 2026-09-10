<script lang="ts">
  import { resolve } from "$app/paths";
  import { imageProxySrcSet, imageProxyUrl } from "@manga-server/api-client/urls";
  import { Badge } from "$lib/ui/badge";
  import { t } from "$lib/i18n";
  import type { UpdateFeedItem } from "$lib/features/updates/types";
  import { formatChapterNumber } from "$lib/utils";

  const THUMBNAIL_SRC_SET_WIDTHS = [56, 112, 160] as const;

  let {
    item,
    categoryLabel,
  } = $props<{
    item: UpdateFeedItem;
    categoryLabel: string;
  }>();

  const coverUrl = $derived(item.manga.cover_proxy_url ?? item.manga.cover_url);
</script>

<a
  href={resolve(`/library/${item.manga.id}` as `/library/${string}`)}
  class="flex items-center gap-4 rounded-xl border border-border/70 bg-card/70 px-3 py-3 transition-colors hover:bg-card"
  data-sveltekit-preload-code="viewport"
>
  <img
    src={imageProxyUrl(coverUrl, item.manga.source_base_url, { format: "avif", width: 112 })}
    srcset={imageProxySrcSet(coverUrl, item.manga.source_base_url, THUMBNAIL_SRC_SET_WIDTHS, { format: "avif" })}
    alt={item.manga.title}
    class="h-20 w-14 shrink-0 rounded-md object-cover"
    decoding="async"
    fetchpriority="low"
    height="160"
    loading="lazy"
    sizes="56px"
    width="112"
  />

  <div class="min-w-0 flex-1">
    <p class="truncate text-lg font-medium text-foreground">{item.manga.title}</p>

    <div class="mt-1 flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
      <span>{$t("app.downloads.chapterWithNumber", { number: formatChapterNumber(item.chapter.chapter_number) })}</span>
      <span>{$t("app.updates.released", { time: item.releaseTimeLabel })}</span>
      {#if item.pickedUpAt}
        <span>{$t("app.updates.pickedUp", { time: item.pickedUpTimeLabel })}</span>
      {/if}
    </div>

    <div class="mt-2 flex flex-wrap items-center gap-2">
      <Badge
        variant="secondary"
        class={item.chapter.downloaded
          ? "border-none bg-primary/15 text-primary"
          : "border-none bg-amber-500/15 text-amber-500"}
      >
        {item.chapter.downloaded ? $t("app.updates.downloaded") : $t("app.updates.new")}
      </Badge>

      {#if categoryLabel}
        <Badge variant="outline" class="text-xs text-muted-foreground">
          {categoryLabel}
        </Badge>
      {/if}
    </div>
  </div>
</a>
