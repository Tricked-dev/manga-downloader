<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import { page } from "$app/state";
  let { data } = $props();

  function loginWithOIDC() {
    const next = page.url.searchParams.get("next") ?? "/";
    window.location.assign(`/auth/login?next=${encodeURIComponent(next)}`);
  }
</script>

<svelte:head>
  <title>{$t("app.login.title")} | {$t("app.appLogo.title")}</title>
</svelte:head>

<Card class="border-border/80 shadow-sm">
  <CardHeader class="text-center pb-2">
    <CardTitle class="text-2xl font-bold">{$t("app.appLogo.title")}</CardTitle>
    <CardDescription>{$t("app.login.description")}</CardDescription>
  </CardHeader>

  <CardContent class="pt-4">
    {#if !data.auth.oidcConfigured}
      <p class="mb-4 text-sm text-muted-foreground">Sign-in is not configured. Contact the server administrator.</p>
    {/if}

    <Button
      size="lg"
      onclick={loginWithOIDC}
      disabled={!data.auth.oidcConfigured}
      class="w-full"
    >
      {$t("app.actions.signInWithSso")}
    </Button>

  </CardContent>
</Card>
