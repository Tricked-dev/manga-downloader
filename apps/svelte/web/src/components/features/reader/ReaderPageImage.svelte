<script lang="ts">
  import { t } from "$lib/i18n";

  let {
    altPage,
    class: className = "w-full",
    fetchPriority = "low",
    imageUrl,
    loading = "lazy",
    onAdvance,
    onImageError,
    onPageLoad,
    pageIndex,
    pageUrl,
    placeholderHeight = "",
    render = true,
  } = $props<{
    altPage: number;
    class?: string;
    fetchPriority?: "high" | "low" | "auto";
    imageUrl: string;
    loading?: "eager" | "lazy";
    onAdvance?: () => void;
    onImageError: (pageUrl: string) => void;
    onPageLoad?: (index: number, event: Event) => void;
    pageIndex: number;
    pageUrl: string;
    placeholderHeight?: string;
    render?: boolean;
  }>();

  function handleError() {
    onImageError(pageUrl);
  }

  function handleLoad(event: Event) {
    onPageLoad?.(pageIndex, event);
  }
</script>

{#if render}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <img
    src={imageUrl}
    alt={$t("app.reader.pageAlt", { page: altPage })}
    class={className}
    decoding="async"
    fetchpriority={fetchPriority}
    loading={loading}
    onclick={onAdvance}
    onerror={handleError}
    onload={handleLoad}
    sizes="min(100vw, 48rem)"
  />
{:else}
  <div
    aria-hidden="true"
    class="w-full bg-muted/20"
    style:height={placeholderHeight}
  ></div>
{/if}
