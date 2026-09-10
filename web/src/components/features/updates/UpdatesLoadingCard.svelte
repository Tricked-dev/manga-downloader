<script lang="ts">
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { Skeleton } from "$lib/ui/skeleton";
  import { t } from "$lib/i18n";

  const LOADING_ROWS = Array.from({ length: 6 }, (_, index) => index);
</script>

<Card class="gap-0 overflow-hidden">
  <CardHeader class="border-b border-border/70">
    <CardTitle>{$t("app.updates.recentReleases")}</CardTitle>
    <CardDescription>{$t("app.updates.recentReleasesLoading")}</CardDescription>
  </CardHeader>
  <CardContent class="space-y-3 p-4">
    {#each LOADING_ROWS as index (index)}
      <div class="flex items-center gap-4 rounded-xl border border-border/70 bg-card/70 p-3">
        <Skeleton class="h-20 w-14 rounded-md" />
        <div class="min-w-0 flex-1 space-y-2">
          <Skeleton class="h-5 w-2/3" />
          <Skeleton class="h-4 w-1/3" />
          <Skeleton class="h-4 w-1/4" />
        </div>
      </div>
    {/each}
  </CardContent>
</Card>
