<script lang="ts">
  import type { StatsRecentChapter } from "@manga-server/api-client/generated/model";
  import { Badge } from "$lib/ui/badge";
  import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import { formatDateLabel } from "$lib/utils";

  let {
    chapters,
  } = $props<{
    chapters: StatsRecentChapter[];
  }>();
</script>

<Card>
  <CardHeader>
    <CardTitle class="text-base">{$t("app.stats.recentReads")}</CardTitle>
    <CardDescription>{$t("app.stats.recentReadsDescription")}</CardDescription>
  </CardHeader>
  <CardContent>
    {#if chapters.length > 0}
      <div class="grid gap-2">
        {#each chapters as chapter (chapter.chapter_id)}
          <div class="grid gap-3 border border-border/70 bg-background px-3 py-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
            <div class="min-w-0">
              <p class="truncate text-sm font-medium">{chapter.manga_title}</p>
              <p class="mt-1 truncate text-xs text-muted-foreground">{chapter.chapter_title}</p>
            </div>
            <div class="flex flex-wrap items-center gap-2 md:justify-end">
              <Badge variant={chapter.read_completed ? "success" : "secondary"}>
                {chapter.read_completed ? $t("app.stats.read") : $t("app.stats.page", { page: chapter.pages_read })}
              </Badge>
              <Badge variant="outline">{chapter.source}</Badge>
              <span class="text-xs text-muted-foreground">{formatDateLabel(chapter.last_read_at)}</span>
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <p class="text-sm text-muted-foreground">{$t("app.stats.noReads")}</p>
    {/if}
  </CardContent>
</Card>
