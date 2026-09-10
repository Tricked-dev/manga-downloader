<script lang="ts">
  import { base } from "$app/paths";
  import { imageProxySrcSet, imageProxyUrl } from "@manga-server/api-client/urls";
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";

  const coverSrcSetWidths = [160, 224, 256, 320, 384, 448] as const;

  type MangaCardManga = {
    author?: string | null;
    category?: string;
    cover_proxy_url?: string | null;
    cover_url: string;
    source_base_url?: string | null;
    status?: string | null;
    title: string;
  };

  const {
    manga,
    href,
    meta,
    footerHref,
    footerLabel,
    details,
    coverFormat,
    fetchPriority = "auto",
    imageLoading = "lazy",
  } = $props<{
    manga: MangaCardManga;
    href: string;
    meta?: string;
    footerHref?: string;
    footerLabel?: string;
    details?: () => unknown;
    coverFormat?: "avif";
    fetchPriority?: "high" | "low" | "auto";
    imageLoading?: "lazy" | "eager";
  }>();

  const coverSource = $derived(manga.cover_proxy_url ?? manga.cover_url);
  let failedCoverSource = $state<string | null>(null);
  const coverFailed = $derived(failedCoverSource === coverSource);
  const coverInitial = $derived(manga.title.trim().charAt(0).toUpperCase() || "?");

  function markCoverFailed() {
    failedCoverSource = coverSource;
  }
</script>

<article
  class="motion-interactive motion-panel overflow-hidden border border-border/80 bg-card [contain-intrinsic-size:420px] [content-visibility:auto] shadow-sm hover:-translate-y-1 hover:shadow-md"
>
  <a
    href={href.startsWith("/") ? `${base}${href}` : href}
    class="group relative block"
    data-sveltekit-preload-code="viewport"
  >
    <div class="aspect-[2/3] overflow-hidden">
      {#if coverFailed}
        <div class="flex h-full w-full items-center justify-center bg-muted text-4xl font-semibold text-muted-foreground">
          {coverInitial}
        </div>
      {:else}
        <img alt={manga.title}
          src={imageProxyUrl(coverSource, manga.source_base_url, {
            format: coverFormat,
          })}
          srcset={coverFormat
            ? imageProxySrcSet(
                coverSource,
                manga.source_base_url,
                coverSrcSetWidths,
                { format: coverFormat },
              )
            : undefined}
          class="h-full w-full object-cover transition-transform duration-500 group-hover:scale-[1.03] motion-reduce:transition-none motion-reduce:group-hover:scale-100"
          decoding="async"
          fetchpriority={fetchPriority}
          height="480"
          loading={imageLoading}
          onerror={markCoverFailed}
          sizes="(min-width: 1536px) 180px, (min-width: 1280px) 16vw, (min-width: 1024px) 19vw, (min-width: 768px) 23vw, (min-width: 640px) 30vw, 45vw"
          width="320"
        />
      {/if}
      <div class="absolute inset-x-0 bottom-0 h-20 bg-gradient-to-t from-black/30 to-transparent opacity-0 transition-opacity duration-200 group-hover:opacity-100"></div>
    </div>

    <div class="space-y-1.5 p-3">
      <h2 class="line-clamp-2 text-sm font-semibold leading-tight">{manga.title}</h2>
      <p class="line-clamp-1 text-xs text-muted-foreground">{manga.author || meta}</p>

      {#if details}
        <div>
          {@render details()}
        </div>
      {/if}
    </div>

    {#if manga.status}
      <div class="absolute right-2 top-2">
        <Badge variant="secondary" class="border-border/0 bg-background/90 text-[10px] font-medium shadow-sm hover:bg-background/90">
          {manga.status}
        </Badge>
      </div>
    {/if}
  </a>

  {#if footerHref && footerLabel}
    <div class="border-t border-border/70 px-3 py-2">
      <Button variant="outline" size="sm" class="h-8 w-full" href={footerHref}>
        {footerLabel}
      </Button>
    </div>
  {/if}
</article>
