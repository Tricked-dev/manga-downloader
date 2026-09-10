<script lang="ts">
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import { cn } from "$lib/utils";
  import type { DetailItem } from "$lib/features/about/types";
  import { MonitorSmartphone, Server, ShieldCheck } from "@lucide/svelte";

  let {
    frontendDetails,
    serverDetails,
  } = $props<{
    frontendDetails: DetailItem[];
    serverDetails: DetailItem[];
  }>();
</script>

<Card class="h-full gap-0 overflow-hidden">
  <CardHeader class="border-b border-border/70 py-4">
    <CardTitle class="flex items-center gap-2">
      <ShieldCheck class="h-5 w-5 text-primary" />
      {$t("app.about.runtime.title")}
    </CardTitle>
    <CardDescription>{$t("app.about.runtime.description")}</CardDescription>
  </CardHeader>

  <CardContent class="grid gap-4 p-4 xl:grid-cols-2">
    <div class="space-y-2.5">
      <div class="flex items-center gap-2 text-sm font-medium text-foreground">
        <MonitorSmartphone class="h-4 w-4 text-primary" />
        {$t("app.about.frontend")}
      </div>
      <div class="divide-y divide-border/70 border border-border/70 bg-muted/20">
        {#each frontendDetails as item (item.label)}
          <div class="grid gap-1.5 p-3.5 sm:grid-cols-[8rem_minmax(0,1fr)] sm:gap-4">
            <p class="text-xs uppercase tracking-[0.18em] text-muted-foreground sm:pt-0.5">{item.label}</p>
            <div class="min-w-0">
              <p class={cn(
                "leading-relaxed text-foreground",
                item.mono ? "break-words font-mono text-sm" : "text-sm font-medium",
              )}>
                {item.value}
              </p>
              {#if item.detail}
                <p class="mt-1 text-xs text-muted-foreground">{item.detail}</p>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>

    <div class="space-y-2.5">
      <div class="flex items-center gap-2 text-sm font-medium text-foreground">
        <Server class="h-4 w-4 text-primary" />
        {$t("app.about.backend")}
      </div>
      <div class="divide-y divide-border/70 border border-border/70 bg-muted/20">
        {#each serverDetails as item (item.label)}
          <div class="grid gap-1.5 p-3.5 sm:grid-cols-[8rem_minmax(0,1fr)] sm:gap-4">
            <p class="text-xs uppercase tracking-[0.18em] text-muted-foreground sm:pt-0.5">{item.label}</p>
            <div class="min-w-0">
              <p class={cn(
                "leading-relaxed text-foreground",
                item.mono ? "break-words font-mono text-sm" : "text-sm font-medium",
              )}>
                {item.value}
              </p>
              {#if item.detail}
                <p class="mt-1 text-xs text-muted-foreground">{item.detail}</p>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
  </CardContent>
</Card>
