<script lang="ts">
  import { AlertTriangle } from "@lucide/svelte";
  import { Button } from "$lib/ui/button";
  import { t } from "$lib/i18n";
  import { getErrorMessage } from "$lib/utils";

  const { error, status } = $props<{ error: (App.Error & { message?: string }) | undefined; status: number }>();
  const normalizedStatus = $derived(status || 500);
  const genericMessage = $derived(getGenericMessage());
  const message = $derived(getErrorMessage(error, genericMessage));
  const title = $derived(getStatusTitle(normalizedStatus));

  function getGenericMessage() {
    return $t("app.errors.generic");
  }

  function getStatusTitle(statusCode: number) {
    if (statusCode === 404) {
      return $t("app.errors.notFound");
    }
    if (statusCode === 401) {
      return $t("app.errors.authRequired");
    }
    if (statusCode >= 500) {
      return $t("app.errors.application");
    }
    return $t("app.errors.status", { status: statusCode });
  }
</script>

<svelte:head>
  <title>{title} | {$t("app.appLogo.title")}</title>
</svelte:head>

<div class="flex min-h-screen items-center justify-center px-4 py-10">
  <div class="w-full max-w-xl border border-border/80 bg-card p-8 text-center">
    <div class="mx-auto mb-4 flex h-14 w-14 items-center justify-center bg-muted text-muted-foreground">
      <AlertTriangle class="h-7 w-7" />
    </div>
    <p class="text-sm uppercase tracking-[0.2em] text-muted-foreground">{$t("app.errors.status", { status: normalizedStatus })}</p>
    <h1 class="mt-2 text-3xl font-semibold tracking-tight">{title}</h1>
    <p class="mt-3 text-sm text-muted-foreground">{message}</p>
    <div class="mt-6 flex flex-wrap justify-center gap-3">
      <Button href="/">{$t("app.errors.goToLibrary")}</Button>
      <Button href="/sources" variant="outline">{$t("app.errors.browseSources")}</Button>
    </div>
  </div>
</div>
