<script lang="ts">
  import { page } from "$app/state";
  import { t } from "$lib/i18n";
  import { isPublicShareViewActive } from "$lib/public-share";
  import AppSidebar from "./AppSidebar.svelte";
  import PublicShareControls from "./PublicShareControls.svelte";
  import type { PublicShareAuthState } from "$lib/public-share";

  const { auth = null, children } = $props<{
    auth?: PublicShareAuthState | null;
    children: () => unknown;
  }>();

  const publicView = $derived(isPublicShareViewActive(auth, page.url));
</script>

<div class={publicView ? "min-h-screen bg-background" : "min-h-screen bg-background lg:grid lg:grid-cols-[15rem_minmax(0,1fr)]"}>
  <a
    href="#content"
    class="sr-only focus:not-sr-only focus:absolute focus:left-4 focus:top-4 focus:z-50 focus:border focus:border-border focus:bg-background focus:px-3 focus:py-2"
  >
    {$t("app.aria.skipToContent")}
  </a>

  {#if !publicView}
    <AppSidebar />
  {/if}

  <div class="min-w-0">
    <main id="content" class="px-4 py-5 sm:px-6 lg:px-8 lg:py-7">
      <div class="mx-auto w-full max-w-7xl">
        <PublicShareControls {auth} showPublishControls={false} />
        {@render children()}
      </div>
    </main>
  </div>
</div>
