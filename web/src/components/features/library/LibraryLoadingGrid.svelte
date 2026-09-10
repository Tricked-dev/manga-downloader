<script lang="ts">
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/ui/card";
  import { Skeleton } from "$lib/ui/skeleton";
  import { t } from "$lib/i18n";

  const LOADING_CARD_IDS = Array.from({ length: 6 }, (_, index) => index);
</script>

<Card class="gap-0">
  <CardHeader class="border-b border-border/70">
    <CardTitle>{$t("app.library.collection")}</CardTitle>
    <div class="pt-1">
      <Skeleton class="h-4 w-64 max-w-full" />
    </div>
  </CardHeader>
  <CardContent class="p-4">
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
      {#each LOADING_CARD_IDS as index (index)}
        <article class="overflow-hidden border border-border/80 bg-card shadow-sm">
          <div class="relative block">
            <Skeleton class="aspect-[2/3] w-full rounded-none" />
            <div class="absolute right-2 top-2">
              <Skeleton class="h-6 w-20 rounded-md" />
            </div>
          </div>

          <div class="space-y-2 p-3">
            <Skeleton class="h-4 w-5/6" />
            <Skeleton class="h-3 w-1/2" />

            <div class="space-y-2 pt-2">
              <Skeleton class="h-3 w-2/3" />
              <Skeleton class="h-3 w-1/2" />
              <Skeleton class="h-5 w-24 rounded-md" />
            </div>
          </div>
        </article>
      {/each}
    </div>
  </CardContent>
</Card>
