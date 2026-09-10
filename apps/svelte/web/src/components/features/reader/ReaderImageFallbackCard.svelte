<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { Card } from "$lib/ui/card";
  import { t } from "$lib/i18n";

  let {
    conflict,
    firstFailedUrl,
    onRetryProxy,
    sourceBaseUrl,
  } = $props<{
    conflict?: boolean;
    firstFailedUrl: string;
    onRetryProxy: () => void;
    sourceBaseUrl: string | null;
  }>();
</script>

<Card class="border-amber-500/40 bg-amber-500/10 p-4">
  <div class="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
    <div class="space-y-1">
      <p class="text-sm font-medium">
        {$t(conflict ? "app.reader.pageConflict" : "app.reader.failedImage")}
      </p>
      <p class="text-sm text-muted-foreground">
        {$t(conflict ? "app.reader.pageConflictDescription" : "app.reader.failedImageDescription")}
      </p>
      {#if firstFailedUrl}
        <p class="break-all text-xs text-muted-foreground">{firstFailedUrl}</p>
      {/if}
    </div>
    <div class="flex flex-wrap gap-2">
      {#if sourceBaseUrl}
        <Button variant="outline" href={sourceBaseUrl} target="_blank" rel="noreferrer">
          {$t("app.reader.openSource")}
        </Button>
      {/if}
      <Button variant="secondary" onclick={onRetryProxy}>{$t("app.reader.proxyRetry")}</Button>
    </div>
  </div>
</Card>
