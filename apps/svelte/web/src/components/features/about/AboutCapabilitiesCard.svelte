<script lang="ts">
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import type { CapabilityItem } from "$lib/features/about/types";
  import { Activity, HardDriveDownload, PackageCheck, PlugZap } from "@lucide/svelte";

  let { items } = $props<{ items: CapabilityItem[] }>();
</script>

<Card class="gap-0 overflow-hidden">
  <CardHeader class="border-b border-border/70 py-4">
    <CardTitle class="flex items-center gap-2">
      <PackageCheck class="h-5 w-5 text-primary" />
      {$t("app.about.capabilities.title")}
    </CardTitle>
    <CardDescription>{$t("app.about.capabilities.description")}</CardDescription>
  </CardHeader>

  <CardContent class="p-4">
    <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
      {#each items as item, index (item.title)}
        <div class="border border-border/70 bg-muted/20 p-3.5">
          <div class="mb-3 flex h-9 w-9 items-center justify-center bg-primary/10 text-primary">
            {#if index === 0}
              <HardDriveDownload class="h-4 w-4" />
            {:else if index === 1}
              <PlugZap class="h-4 w-4" />
            {:else if index === 2}
              <Activity class="h-4 w-4" />
            {:else}
              <PackageCheck class="h-4 w-4" />
            {/if}
          </div>
          <h2 class="text-sm font-medium text-foreground">{item.title}</h2>
          <p class="mt-1.5 text-sm leading-6 text-muted-foreground">{item.description}</p>
        </div>
      {/each}
    </div>
  </CardContent>
</Card>
