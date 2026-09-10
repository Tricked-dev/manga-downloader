<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import type { LinkItem } from "$lib/features/about/types";
  import { ExternalLink, Globe } from "@lucide/svelte";

  let { items } = $props<{ items: LinkItem[] }>();
</script>

<Card class="gap-0 overflow-hidden">
  <CardHeader class="border-b border-border/70 py-4">
    <CardTitle class="flex items-center gap-2">
      <Globe class="h-5 w-5 text-primary" />
      {$t("app.about.links.title")}
    </CardTitle>
    <CardDescription>{$t("app.about.links.description")}</CardDescription>
  </CardHeader>

  <CardContent class="p-4">
    {#if items.length === 0}
      <div class="border border-dashed border-border/80 bg-muted/20 px-4 py-7 text-center">
        <Globe class="mx-auto h-8 w-8 text-muted-foreground" />
        <p class="mt-3 text-sm font-medium text-foreground">{$t("app.about.links.emptyTitle")}</p>
        <p class="mt-1 text-sm leading-6 text-muted-foreground">
          {$t("app.about.links.emptyDescription")}
        </p>
      </div>
    {:else}
      <div class="grid gap-3">
        {#each items as item (item.label)}
          <div class="border border-border/70 bg-muted/20 p-3.5">
            <div class="flex flex-col gap-3">
              <div class="min-w-0">
                <p class="flex items-center gap-2 text-xs uppercase tracking-[0.18em] text-muted-foreground">
                  {#if item.id === "repository"}
                    <svg
                      viewBox="0 0 24 24"
                      aria-hidden="true"
                      class="h-4 w-4 fill-current"
                    >
                      <path d="M12 .5C5.65.5.5 5.66.5 12.02c0 5.09 3.29 9.4 7.86 10.93.58.11.79-.25.79-.56 0-.27-.01-1.18-.02-2.14-3.2.7-3.87-1.36-3.87-1.36-.52-1.34-1.28-1.69-1.28-1.69-1.05-.71.08-.7.08-.7 1.16.08 1.78 1.2 1.78 1.2 1.03 1.77 2.7 1.26 3.36.96.1-.75.4-1.26.73-1.55-2.55-.29-5.23-1.28-5.23-5.71 0-1.26.45-2.3 1.19-3.11-.12-.29-.52-1.47.11-3.06 0 0 .97-.31 3.19 1.19a11.02 11.02 0 0 1 5.8 0c2.21-1.5 3.18-1.19 3.18-1.19.63 1.59.23 2.77.12 3.06.74.81 1.18 1.85 1.18 3.11 0 4.45-2.69 5.41-5.25 5.7.41.36.78 1.07.78 2.16 0 1.56-.01 2.81-.01 3.19 0 .31.21.68.8.56a11.53 11.53 0 0 0 7.84-10.93C23.5 5.66 18.35.5 12 .5Z" />
                    </svg>
                  {/if}
                  {item.label}
                </p>
                <p class="mt-2 break-all text-sm leading-6 text-foreground">{item.value}</p>
              </div>

              <Button
                href={item.href}
                target="_blank"
                rel="noreferrer"
                variant="outline"
                size="sm"
                class="w-fit gap-2"
              >
                <ExternalLink class="h-4 w-4" />
                {$t("app.about.links.open")}
              </Button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </CardContent>
</Card>
