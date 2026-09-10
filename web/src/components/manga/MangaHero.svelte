<script lang="ts">
  import { imageProxySrcSet, imageProxyUrl } from "@manga-server/api-client/urls";
  import { ArrowLeft } from "@lucide/svelte";
  import { Badge } from "$lib/ui/badge";
  import { Button } from "$lib/ui/button";
  import { t } from "$lib/i18n";

  const coverSrcSetWidths = [192, 224, 256, 320, 384, 448] as const;

  const {
    title,
    coverUrl,
    coverBaseUrl,
    author,
    status,
    description = "",
    backHref,
    backLabel = "",
    headerActions,
    details,
    actions,
  } = $props<{
    title: string;
    coverUrl: string;
    coverBaseUrl?: string | null;
    author?: string;
    status?: string;
    description?: string;
    backHref?: string;
    backLabel?: string;
    headerActions?: () => unknown;
    details?: () => unknown;
    actions?: () => unknown;
  }>();

  let descriptionExpanded = $state(false);
  const descriptionClass = $derived(
    `mt-5 max-w-3xl text-sm leading-7 text-muted-foreground ${descriptionExpanded ? "" : "line-clamp-2"}`
  );

  function toggleDescription() {
    descriptionExpanded = !descriptionExpanded;
  }
</script>

<div class="border border-border/80 bg-card p-5 shadow-sm sm:p-6">
  {#if backHref || headerActions}
    <div class="mb-4 flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
      {#if backHref}
        <Button href={backHref} variant="ghost" class="-ml-2 gap-1 text-muted-foreground">
          <ArrowLeft class="h-4 w-4" />
          {backLabel || $t("app.actions.back")}
        </Button>
      {/if}

      {#if headerActions}
        <div class="min-w-0 sm:ml-auto">
          {@render headerActions()}
        </div>
      {/if}
    </div>
  {/if}

  <div class="flex flex-col gap-8 lg:flex-row">
    <div class="flex-shrink-0">
      <img alt={title}
        src={imageProxyUrl(coverUrl, coverBaseUrl, { format: "avif" })}
        srcset={imageProxySrcSet(coverUrl, coverBaseUrl, coverSrcSetWidths, {
          format: "avif",
        })}
        class="mx-auto w-44 border border-border/70 shadow-sm sm:w-56 lg:mx-0"
        decoding="async"
        fetchpriority="high"
        height="576"
        sizes="(min-width: 640px) 224px, 192px"
        width="384"
      />
    </div>

    <div class="flex-1">
      <h1 class="text-2xl font-semibold tracking-tight sm:text-4xl">{title}</h1>

      <div class="mt-4 flex flex-wrap gap-2">
        {#if status}
          <Badge variant="outline" class="bg-muted/70">
            {status}
          </Badge>
        {/if}

        {#if author}
          <Badge variant="secondary">
            {author}
          </Badge>
        {/if}
      </div>

      {#if description}
        <div>
          <p class={descriptionClass}>{description}</p>
          <Button
            variant="ghost"
            size="sm"
            class="-ml-2 mt-1 h-7 px-2 text-xs text-muted-foreground"
            aria-expanded={descriptionExpanded}
            onclick={toggleDescription}
          >
            {descriptionExpanded ? $t("app.common.showLess") : $t("app.common.showMore")}
          </Button>
        </div>
      {/if}

      {#if details}
        <div class="mt-5 max-w-5xl">
          {@render details()}
        </div>
      {/if}

      {#if actions}
        <div class="mt-6 border-t border-border/70 pt-6">
          {@render actions()}
        </div>
      {/if}
    </div>
  </div>
</div>
