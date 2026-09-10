<script lang="ts">
  import { Card, CardContent, CardHeader, CardTitle } from "$lib/ui/card";
  import { Skeleton } from "$lib/ui/skeleton";

  const LEFT_CARDS = Array.from({ length: 5 }, (_, index) => index);
  const RIGHT_CARDS = Array.from({ length: 3 }, (_, index) => index);
  const IMAGE_ROWS = Array.from({ length: 2 }, (_, index) => index);
</script>

<div class="grid gap-3 xl:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)] xl:items-start">
  <div class="space-y-3">
    {#each LEFT_CARDS as index (index)}
      <Card size="sm" class="overflow-visible">
        <CardHeader class="pb-3">
          <CardTitle><Skeleton class="h-5 w-40" /></CardTitle>
          <div class="pt-1">
            <Skeleton class="h-4 w-64 max-w-full" />
          </div>
        </CardHeader>
        <CardContent>
          <div class="space-y-4">
            <div class="space-y-2">
              <Skeleton class="h-4 w-1/3" />
              <Skeleton class="h-9 w-full rounded-md" />
            </div>
            <div class="space-y-2">
              <Skeleton class="h-4 w-1/2" />
              <Skeleton class="h-9 w-full rounded-md" />
            </div>
            <div class="flex items-center justify-between gap-4 rounded-md border border-border/60 bg-background/20 p-3">
              <div class="space-y-2">
                <Skeleton class="h-4 w-28" />
                <Skeleton class="h-3 w-40" />
              </div>
              <Skeleton class="h-6 w-10 rounded-full" />
            </div>
          </div>
        </CardContent>
      </Card>
    {/each}
  </div>

  <div class="space-y-3">
    {#each RIGHT_CARDS as index (index)}
      <Card size="sm" class="overflow-visible">
        <CardHeader class="pb-3">
          <CardTitle><Skeleton class="h-5 w-44" /></CardTitle>
          <div class="pt-1">
            <Skeleton class="h-4 w-72 max-w-full" />
          </div>
        </CardHeader>
        <CardContent>
          <div class="space-y-4">
            <div class="space-y-2">
              <Skeleton class="h-4 w-1/3" />
              <Skeleton class="h-9 w-full rounded-md" />
            </div>
            <div class="space-y-2">
              <div class="flex items-center justify-between gap-4">
                <Skeleton class="h-4 w-24" />
                <Skeleton class="h-4 w-10" />
              </div>
              <Skeleton class="h-2 w-full rounded-full" />
            </div>
            <Skeleton class="h-9 w-40 rounded-md" />
          </div>
        </CardContent>
      </Card>
    {/each}

    <Card size="sm" class="overflow-visible">
      <CardHeader class="pb-3">
        <CardTitle><Skeleton class="h-5 w-40" /></CardTitle>
        <div class="pt-1">
          <Skeleton class="h-4 w-72 max-w-full" />
        </div>
      </CardHeader>
      <CardContent>
        <div class="space-y-3">
          {#each IMAGE_ROWS as index (index)}
            <div class="space-y-3 rounded-md border border-border/70 bg-card/50 p-3.5">
              <div class="flex items-center justify-between gap-4">
                <div class="space-y-2">
                  <Skeleton class="h-4 w-28" />
                  <Skeleton class="h-3 w-48" />
                </div>
                <Skeleton class="h-6 w-16 rounded-md" />
              </div>
              <div class="flex items-center justify-between gap-4">
                <div class="space-y-2">
                  <Skeleton class="h-4 w-32" />
                  <Skeleton class="h-3 w-44" />
                </div>
                <Skeleton class="h-6 w-10 rounded-full" />
              </div>
              <div class="space-y-2.5 rounded-md border border-border/60 bg-background/30 p-3">
                <div class="flex items-center justify-between gap-4">
                  <Skeleton class="h-4 w-24" />
                  <Skeleton class="h-4 w-10" />
                </div>
                <Skeleton class="h-2 w-full rounded-full" />
              </div>
            </div>
          {/each}
        </div>
      </CardContent>
    </Card>
  </div>
</div>
