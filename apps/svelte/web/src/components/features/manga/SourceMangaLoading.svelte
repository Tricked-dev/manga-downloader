<script lang="ts">
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { Skeleton } from "$lib/ui/skeleton";
  import { t } from "$lib/i18n";

  const CHAPTER_SKELETON_ROWS = Array.from({ length: 8 }, (_, index) => index);

  let { sourceName } = $props<{ sourceName: string }>();
</script>

<Card class="gap-0 overflow-hidden">
  <CardContent class="p-0">
    <div class="grid gap-6 p-5 md:grid-cols-[12rem_minmax(0,1fr)] lg:grid-cols-[15rem_minmax(0,1fr)] lg:p-6">
      <Skeleton class="aspect-[2/3] w-full max-w-60 rounded-md" />
      <div class="min-w-0 space-y-5">
        <div class="space-y-3">
          <Skeleton class="h-8 w-3/4 max-w-xl" />
          <div class="flex flex-wrap gap-2">
            <Skeleton class="h-6 w-20 rounded-md" />
            <Skeleton class="h-6 w-28 rounded-md" />
          </div>
        </div>
        <div class="space-y-2">
          <Skeleton class="h-4 w-full" />
          <Skeleton class="h-4 w-11/12" />
          <Skeleton class="h-4 w-4/5" />
          <Skeleton class="h-4 w-2/3" />
        </div>
        <div class="flex flex-wrap items-end gap-3">
          <div class="w-full max-w-xs space-y-2">
            <Skeleton class="h-3 w-24" />
            <Skeleton class="h-10 w-full rounded-md" />
          </div>
          <Skeleton class="h-11 w-36 rounded-md" />
        </div>
      </div>
    </div>
  </CardContent>
</Card>

<Card class="gap-0">
  <CardHeader class="border-b border-border/70">
    <CardTitle>{$t("app.manga.chapters")}</CardTitle>
    <CardDescription>{$t("app.manga.loadingChaptersFrom", { source: sourceName })}</CardDescription>
  </CardHeader>
  <CardContent class="p-0">
    <div class="divide-y divide-border">
      {#each CHAPTER_SKELETON_ROWS as index (index)}
        <div class="flex flex-col gap-3 px-5 py-4 lg:flex-row lg:items-center lg:justify-between">
          <div class="min-w-0 flex-1 space-y-2">
            <Skeleton class="h-4 w-40" />
            <div class="flex flex-wrap gap-2">
              <Skeleton class="h-6 w-24 rounded-md" />
              <Skeleton class="h-6 w-16 rounded-md" />
            </div>
          </div>
          <div class="flex items-center gap-2">
            <Skeleton class="h-8 w-16 rounded-md" />
            <Skeleton class="h-8 w-28 rounded-md" />
          </div>
        </div>
      {/each}
    </div>
  </CardContent>
</Card>
