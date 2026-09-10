<script lang="ts">
  import UpscaleStatus from "$components/downloads/UpscaleStatus.svelte";
  import DownloadStatusBadge from "$components/downloads/DownloadStatusBadge.svelte";
  import { ACTIVE_DOWNLOAD_STATUSES } from "$lib/features/downloads/status";
  import { Badge } from "$lib/ui/badge";
  import { t } from "$lib/i18n";
  import type { DownloadItem } from "$lib/types";

  let {
    chapterDownloaded,
    download,
  } = $props<{
    chapterDownloaded: boolean;
    download: DownloadItem | undefined;
  }>();
</script>

{#if download}
  <div class="flex items-center gap-2">
    <DownloadStatusBadge status={download.status} />
    <UpscaleStatus {download} />
    {#if ACTIVE_DOWNLOAD_STATUSES.has(download.status)}
      <span class="text-xs text-muted-foreground">{Math.round(download.progress)}%</span>
    {/if}
  </div>
{:else if chapterDownloaded}
  <Badge variant="success">{$t("app.manga.downloaded")}</Badge>
{:else}
  <Badge variant="outline">{$t("app.libraryDetail.notDownloaded")}</Badge>
{/if}
