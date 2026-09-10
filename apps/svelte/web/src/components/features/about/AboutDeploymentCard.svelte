<script lang="ts">
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import { cn } from "$lib/utils";
  import type { DetailItem } from "$lib/features/about/types";
  import { Clock3, Code2, GitBranch, GitCommit } from "@lucide/svelte";

  let { items } = $props<{ items: DetailItem[] }>();
</script>

<Card class="gap-0 overflow-hidden">
  <CardHeader class="border-b border-border/70 py-4">
    <CardTitle class="flex items-center gap-2">
      <GitBranch class="h-5 w-5 text-primary" />
      {$t("app.about.deployment.title")}
    </CardTitle>
    <CardDescription>{$t("app.about.deployment.description")}</CardDescription>
  </CardHeader>

  <CardContent class="p-4">
    {#if items.length === 0}
      <div class="border border-dashed border-border/80 bg-muted/20 px-4 py-7 text-center">
        <GitBranch class="mx-auto h-8 w-8 text-muted-foreground" />
        <p class="mt-3 text-sm font-medium text-foreground">{$t("app.about.deployment.emptyTitle")}</p>
        <p class="mt-1 text-sm leading-6 text-muted-foreground">
          {$t("app.about.deployment.emptyDescription")}
        </p>
      </div>
    {:else}
      <div class="divide-y divide-border/70 border border-border/70 bg-muted/20">
        {#each items as item (item.label)}
          <div class="flex items-start gap-3 p-3.5">
            <div class="mt-0.5 text-muted-foreground">
              {#if item.icon === "lastCommit"}
                <GitCommit class="h-4 w-4" />
              {:else if item.icon === "remoteCommit"}
                <Code2 class="h-4 w-4" />
              {:else}
                <GitBranch class="h-4 w-4" />
              {/if}
            </div>
            <div class="min-w-0">
              <p class="text-xs uppercase tracking-[0.18em] text-muted-foreground">{item.label}</p>
              {#if item.value}
                <p class={cn(
                  "mt-1 leading-relaxed text-foreground",
                  item.mono ? "break-words font-mono text-sm" : "text-sm font-medium",
                )}>
                  {item.value}
                </p>
              {/if}
              {#if item.detail}
                <p class="mt-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                  <Clock3 class="h-3.5 w-3.5" />
                  {item.detail}
                </p>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </CardContent>
</Card>
