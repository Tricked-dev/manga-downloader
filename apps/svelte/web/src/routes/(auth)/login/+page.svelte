<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import { getErrorMessage } from "$lib/utils";

  let loginError = $state("");

  async function loginWithOIDC() {
    try {
      const response = await fetch("/api/auth/sign-in/oauth2", {
        body: JSON.stringify({
          callbackURL: "/",
          providerId: "oidc",
        }),
        credentials: "same-origin",
        headers: {
          "content-type": "application/json",
        },
        method: "POST",
      });
      const payload = (await response.json().catch(() => null)) as { url?: unknown } | null;
      if (!response.ok || typeof payload?.url !== "string") {
        throw new Error($t("app.errors.genericShort"));
      }
      globalThis.location.href = payload.url;
    } catch (error: unknown) {
      loginError = getErrorMessage(error, $t("app.errors.genericShort"));
    }
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
    {#if loginError}
      <div class="mb-4 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3">
        <p class="text-sm text-destructive">{loginError}</p>
      </div>
    {/if}

    <Button
      size="lg"
      onclick={loginWithOIDC}
      class="w-full"
    >
      {$t("app.actions.signInWithSso")}
    </Button>

    <p class="mt-4 text-center text-[11px] text-muted-foreground/60">
      {$t("app.login.authPoweredBy")}
    </p>
  </CardContent>
</Card>
