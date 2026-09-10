<script lang="ts">
  import { t } from "$lib/i18n";
  import { cn } from "$lib/utils";
  import type { HealthCheckItem } from "$lib/features/about/types";

  let {
    checks,
    detail,
  } = $props<{
    checks: HealthCheckItem[];
    detail: string;
  }>();

  function formatComponentName(component: string): string {
    return component
      .split("_")
      .filter((part) => part.length > 0)
      .map((part) => part[0]?.toUpperCase() + part.slice(1))
      .join(" ");
  }
</script>

<div class="absolute right-0 top-full z-50 mt-2 hidden w-[min(20rem,calc(100vw-3rem))] border border-border/80 bg-card p-3.5 text-left shadow-lg ring-1 ring-foreground/10 group-hover:block md:right-full md:top-0 md:mr-3 md:mt-0">
  {#if checks.length === 0}
    <p class="text-xs leading-5 text-muted-foreground">
      {$t("app.about.health.noDetails")}
    </p>
  {:else}
    <div class="mb-3 border-b border-border/70 pb-2">
      <p class="text-xs uppercase tracking-[0.18em] text-muted-foreground">{$t("app.about.health.title")}</p>
      <p class="mt-1 text-sm font-medium text-foreground">{detail}</p>
    </div>
    <div class="space-y-2">
      {#each checks as check (check.component)}
        <div class="flex items-start gap-2">
          <span class={cn("mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full", check.ok ? "bg-emerald-400" : "bg-destructive")}></span>
          <div class="min-w-0">
            <p class="text-xs font-medium text-foreground">{formatComponentName(check.component)}</p>
            {#if check.detail}
              <p class="mt-0.5 text-xs leading-5 text-muted-foreground">{check.detail}</p>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
