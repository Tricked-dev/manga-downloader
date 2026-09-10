<script lang="ts">
  import AboutStatusCard from "$components/features/about/AboutStatusCard.svelte";
  import { Card, CardContent } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import type { StatusItem } from "$lib/features/about/types";
  import { BookOpenCheck } from "@lucide/svelte";

  let {
    appName,
    appVersion,
    serverVersion,
    statusItems,
  } = $props<{
    appName: string;
    appVersion: string;
    serverVersion: string;
    statusItems: StatusItem[];
  }>();
</script>

<section>
  <Card class="gap-0 overflow-visible border-border/70">
    <CardContent class="p-0">
      <div class="grid gap-0 md:grid-cols-[minmax(0,1fr)_18rem]">
        <div class="flex flex-col justify-between gap-8 border-b border-border/70 p-5 sm:p-6 md:border-b-0 md:border-r">
          <div class="space-y-4">
            <div class="inline-flex h-11 w-11 items-center justify-center bg-primary text-primary-foreground">
              <BookOpenCheck class="h-5 w-5" />
            </div>
            <div>
              <p class="text-xs uppercase tracking-[0.18em] text-muted-foreground">{appName}</p>
              <h2 class="mt-2 text-3xl font-semibold tracking-tight text-foreground sm:text-4xl">
                {$t("app.about.heroTitle")}
              </h2>
              <p class="mt-3 max-w-5xl text-sm leading-6 text-muted-foreground">
                {$t("app.about.heroDescription")}
              </p>
            </div>
          </div>

          <div class="grid gap-3 sm:grid-cols-2">
            <div class="border border-border/70 bg-muted/20 p-3.5">
              <p class="text-xs uppercase tracking-[0.18em] text-muted-foreground">{$t("app.about.webUi")}</p>
              <p class="mt-2 text-2xl font-semibold text-foreground">v{appVersion}</p>
            </div>
            <div class="border border-border/70 bg-muted/20 p-3.5">
              <p class="text-xs uppercase tracking-[0.18em] text-muted-foreground">{$t("app.about.backend")}</p>
              <p class="mt-2 text-2xl font-semibold text-foreground">v{serverVersion}</p>
            </div>
          </div>
        </div>

        <div class="grid content-start gap-3 p-4 sm:p-5">
          {#each statusItems as item (item.label)}
            <AboutStatusCard {item} />
          {/each}
        </div>
      </div>
    </CardContent>
  </Card>
</section>
