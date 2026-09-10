<script lang="ts">
  import { Moon, Sun } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { themeController } from "$lib/theme.svelte";

  onMount(() => {
    themeController.initialize();
  });

  const nextThemeLabel = $derived(
    themeController.resolved === "dark" ? $t("app.theme.light") : $t("app.theme.dark"),
  );
</script>

<button
  type="button"
  class="inline-flex h-9 w-9 shrink-0 items-center justify-center border border-border bg-background text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
  aria-label={$t("app.theme.switchTheme", { theme: nextThemeLabel })}
  title={$t("app.theme.toggleTheme")}
  onclick={themeController.toggle}
>
  {#if themeController.resolved === "dark"}
    <Moon class="h-4 w-4" />
  {:else}
    <Sun class="h-4 w-4" />
  {/if}
</button>
