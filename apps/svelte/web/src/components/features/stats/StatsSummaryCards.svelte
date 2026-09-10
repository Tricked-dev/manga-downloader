<script lang="ts">
  import type { StatsTotals } from "@manga-server/api-client/generated/model";
  import { Card, CardContent, CardDescription, CardHeader } from "$lib/ui/card";
  import { t } from "$lib/i18n";
  import { BookOpen, Download, FileDown, Library } from "@lucide/svelte";

  let {
    totals,
    formatNumber,
  } = $props<{
    totals: StatsTotals;
    formatNumber: (value?: number) => string;
  }>();
</script>

<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
  <Card>
    <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
      <CardDescription>{$t("app.stats.chaptersRead")}</CardDescription>
      <BookOpen class="size-4 text-muted-foreground" />
    </CardHeader>
    <CardContent>
      <div class="text-3xl font-semibold">{formatNumber(totals.chapters_read)}</div>
      <p class="mt-1 text-xs text-muted-foreground">{$t("app.stats.pagesRead", { count: formatNumber(totals.pages_read) })}</p>
    </CardContent>
  </Card>

  <Card>
    <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
      <CardDescription>{$t("app.stats.chaptersDownloaded")}</CardDescription>
      <FileDown class="size-4 text-muted-foreground" />
    </CardHeader>
    <CardContent>
      <div class="text-3xl font-semibold">{formatNumber(totals.chapters_downloaded)}</div>
      <p class="mt-1 text-xs text-muted-foreground">{$t("app.stats.pagesSaved", { count: formatNumber(totals.pages_downloaded) })}</p>
    </CardContent>
  </Card>

  <Card>
    <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
      <CardDescription>{$t("app.nav.library")}</CardDescription>
      <Library class="size-4 text-muted-foreground" />
    </CardHeader>
    <CardContent>
      <div class="text-3xl font-semibold">{formatNumber(totals.library_series)}</div>
      <p class="mt-1 text-xs text-muted-foreground">{$t("app.stats.libraryBreakdown", { chapters: formatNumber(totals.library_chapters), newChapters: formatNumber(totals.new_chapters) })}</p>
    </CardContent>
  </Card>

  <Card>
    <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-2">
      <CardDescription>{$t("app.stats.activity")}</CardDescription>
      <Download class="size-4 text-muted-foreground" />
    </CardHeader>
    <CardContent>
      <div class="text-3xl font-semibold">{formatNumber(totals.active_downloads)}</div>
      <p class="mt-1 text-xs text-muted-foreground">{$t("app.stats.failedDownloads", { count: formatNumber(totals.failed_downloads) })}</p>
    </CardContent>
  </Card>
</div>
