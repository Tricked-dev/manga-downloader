<script lang="ts">
  import { AlertTriangle } from "@lucide/svelte";
  import { Button } from "$lib/ui/button";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import { getErrorMessage } from "$lib/utils";

  const { error, status } = $props<{ error: (App.Error & { message?: string }) | undefined; status: number }>();
  const authFallbackMessage = $derived(getAuthFallbackMessage());
  const message = $derived(getErrorMessage(error, authFallbackMessage));

  function getAuthFallbackMessage() {
    return $t("app.errors.authFallback", { status });
  }
</script>

<svelte:head>
  <title>{$t("app.errors.status", { status })} | {$t("app.appLogo.title")}</title>
</svelte:head>

<Card class="border-border/80 shadow-sm">
  <CardHeader class="text-center">
    <div class="mx-auto mb-2 flex h-12 w-12 items-center justify-center bg-muted text-muted-foreground">
      <AlertTriangle class="h-6 w-6" />
    </div>
    <CardTitle>{$t("app.errors.auth")}</CardTitle>
    <CardDescription>{message}</CardDescription>
  </CardHeader>
  <CardContent class="flex justify-center">
    <Button href="/" variant="outline">{$t("app.errors.returnToApp")}</Button>
  </CardContent>
</Card>
