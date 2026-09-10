<script lang="ts">
  import { browser } from "$app/environment";
  import { resolve } from "$app/paths";
  import { page } from "$app/state";
  import { Check, Copy, Globe2, Loader2, Share2, X } from "@lucide/svelte";
  import { Button } from "$lib/ui/button";
  import { t } from "$lib/i18n";
  import { isPublicShareViewActive, isShareablePublicPath } from "$lib/public-share";
  import type { PublicShareAuthState } from "$lib/public-share";

  interface ShareStatus {
    public: boolean;
    shareUrl: string | null;
  }

  let {
    auth,
    inline = false,
    showPublicViewBanner = true,
    showPublishControls = true,
  } = $props<{
    auth: PublicShareAuthState | null;
    inline?: boolean;
    showPublicViewBanner?: boolean;
    showPublishControls?: boolean;
  }>();

  let status = $state<ShareStatus | null>(null);
  let loadingStatus = $state(false);
  let busy = $state(false);
  let copied = $state(false);
  let errorMessage = $state("");

  const authenticated = $derived(auth?.enabled === true && auth.authenticated);
  const publicView = $derived(isPublicShareViewActive(auth, page.url));
  const shareable = $derived(isShareablePublicPath(page.url.pathname));
  const routeHref = $derived(page.url.href);
  const canPublish = $derived(authenticated && shareable && !publicView);
  const publishContainerClass = $derived(
    inline ? "flex max-w-full flex-col gap-2 sm:items-end" : "mb-4 flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-end"
  );
  const publishButtonRowClass = $derived(inline ? "flex flex-wrap gap-2 sm:justify-end" : "flex flex-wrap justify-end gap-2");

  $effect(() => {
    if (!browser || !canPublish) {
      status = null;
      loadingStatus = false;
      errorMessage = "";
      return;
    }

    const controller = new AbortController();
    void loadStatus(routeHref, controller.signal);
    return () => controller.abort();
  });

  async function loadStatus(urlToCheck: string, signal?: AbortSignal) {
    loadingStatus = true;
    errorMessage = "";
    try {
      const response = await fetch(publicShareApiUrl(urlToCheck), { signal });
      if (!response.ok) {
        throw new Error($t("app.share.statusFailed"));
      }
      status = (await response.json()) as ShareStatus;
    } catch (caught) {
      if (!(caught instanceof DOMException && caught.name === "AbortError")) {
        status = null;
      }
    } finally {
      if (!signal?.aborted) {
        loadingStatus = false;
      }
    }
  }

  async function publishAndCopy() {
    busy = true;
    copied = false;
    errorMessage = "";
    try {
      const response = await fetch(resolve("/api/app/public-share"), {
        body: JSON.stringify({ url: routeHref }),
        headers: {
          "content-type": "application/json",
        },
        method: "POST",
      });
      if (!response.ok) {
        throw new Error($t("app.share.publishFailed"));
      }

      status = (await response.json()) as ShareStatus;
      await copyPublicUrl(status.shareUrl);
    } catch (caught) {
      errorMessage = caught instanceof Error ? caught.message : $t("app.share.publishFailed");
    } finally {
      busy = false;
    }
  }

  async function copyExisting() {
    busy = true;
    errorMessage = "";
    try {
      await copyPublicUrl(status?.shareUrl ?? auth?.publicAccess?.shareUrl ?? null);
    } catch (caught) {
      errorMessage = caught instanceof Error ? caught.message : $t("app.share.copyFailed");
    } finally {
      busy = false;
    }
  }

  async function unpublish() {
    busy = true;
    copied = false;
    errorMessage = "";
    try {
      const response = await fetch(publicShareApiUrl(routeHref), {
        method: "DELETE",
      });
      if (!response.ok) {
        throw new Error($t("app.share.unpublishFailed"));
      }
      status = {
        public: false,
        shareUrl: null,
      };
    } catch (caught) {
      errorMessage = caught instanceof Error ? caught.message : $t("app.share.unpublishFailed");
    } finally {
      busy = false;
    }
  }

  async function copyPublicUrl(value: string | null) {
    if (!value) {
      throw new Error($t("app.share.copyFailed"));
    }

    await navigator.clipboard.writeText(new URL(value, window.location.origin).href);
    copied = true;
    window.setTimeout(() => {
      copied = false;
    }, 1800);
  }

  function publicShareApiUrl(urlToCheck: string): string {
    const endpoint = new URL(resolve("/api/app/public-share"), window.location.origin);
    endpoint.searchParams.set("url", urlToCheck);
    return endpoint.toString();
  }
</script>

{#if publicView && showPublicViewBanner}
  <div class="mb-4 flex flex-col gap-3 border border-primary/20 bg-primary/5 px-3 py-3 text-sm sm:flex-row sm:items-center sm:justify-between">
    <div class="flex min-w-0 items-start gap-2 text-primary">
      <Globe2 class="mt-0.5 size-4 shrink-0" />
      <div class="min-w-0">
        <p class="font-medium">{$t("app.share.publicView")}</p>
        <p class="text-xs text-muted-foreground">{$t("app.share.publicViewDescription")}</p>
      </div>
    </div>
    {#if auth?.publicAccess?.shareUrl}
      <Button variant="outline" size="sm" onclick={copyExisting} disabled={busy}>
        {#if copied}
          <Check class="size-3.5" />
          {$t("app.share.copied")}
        {:else}
          <Copy class="size-3.5" />
          {$t("app.share.copyLink")}
        {/if}
      </Button>
    {/if}
  </div>
{:else if canPublish && showPublishControls}
  <div class={publishContainerClass}>
    {#if errorMessage}
      <p class="text-xs text-destructive">{errorMessage}</p>
    {/if}
    <div class={publishButtonRowClass}>
      {#if status?.public}
        <Button variant="outline" size="sm" onclick={copyExisting} disabled={busy || loadingStatus}>
          {#if copied}
            <Check class="size-3.5" />
            {$t("app.share.copied")}
          {:else}
            <Copy class="size-3.5" />
            {$t("app.share.copyPublicLink")}
          {/if}
        </Button>
        <Button variant="ghost" size="sm" onclick={unpublish} disabled={busy || loadingStatus}>
          {#if busy}
            <Loader2 class="size-3.5 animate-spin" />
          {:else}
            <X class="size-3.5" />
          {/if}
          {$t("app.share.unpublish")}
        </Button>
      {:else}
        <Button variant="outline" size="sm" onclick={publishAndCopy} disabled={busy || loadingStatus}>
          {#if busy || loadingStatus}
            <Loader2 class="size-3.5 animate-spin" />
            {$t("app.share.preparing")}
          {:else}
            <Share2 class="size-3.5" />
            {$t("app.share.publish")}
          {/if}
        </Button>
      {/if}
    </div>
  </div>
{/if}
