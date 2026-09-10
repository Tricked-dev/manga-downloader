<script lang="ts">
  import LoadingState from "$components/common/LoadingState.svelte";
  import { t } from "$lib/i18n";

  const {
    hasMore,
    label = "",
    onLoadMore,
  } = $props<{
    hasMore: boolean;
    label?: string;
    onLoadMore: () => void;
  }>();
  const resolvedLabel = $derived(label || $t("app.common.loadingMore"));

  let triggerElement: HTMLDivElement | undefined = $state();

  $effect(() => {
    if (!triggerElement || !hasMore) {
      return;
    }

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry?.isIntersecting) {
          onLoadMore();
        }
      },
      { rootMargin: "320px 0px" },
    );

    observer.observe(triggerElement);

    return () => {
      observer.disconnect();
    };
  });
</script>

{#if hasMore}
  <div bind:this={triggerElement} class="border-t border-border/70 px-4 py-4">
    <LoadingState label={resolvedLabel} compact />
  </div>
{/if}
