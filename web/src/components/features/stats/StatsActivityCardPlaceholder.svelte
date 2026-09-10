<script lang="ts">
  import { ChartArea } from "@lucide/svelte";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { Skeleton } from "$lib/ui/skeleton";
  import { t } from "$lib/i18n";

  let { hasActivity }: { hasActivity: boolean } = $props();
</script>

<Card>
  <CardHeader>
    <div class="flex items-center justify-between gap-3">
      <div>
        <CardTitle class="text-base">{$t("app.stats.last30Days")}</CardTitle>
        <CardDescription>{$t("app.stats.last30DaysDescription")}</CardDescription>
      </div>
      <ChartArea class="hidden size-5 text-muted-foreground sm:block" />
    </div>
  </CardHeader>
  <CardContent>
    {#if hasActivity}
      <Skeleton class="h-[280px] min-h-[240px] w-full border border-border/70 bg-muted/30" />
    {:else}
      <div class="flex h-[280px] items-center justify-center border border-dashed border-border/80 bg-muted/20">
        <p class="text-sm text-muted-foreground">{$t("app.stats.noActivity")}</p>
      </div>
    {/if}
  </CardContent>
</Card>
