<script lang="ts">
  import { Badge } from "$lib/ui/badge";
  import { t } from "$lib/i18n";
  import type { DownloadItem } from "$lib/types";
  const { download } = $props<{ download: DownloadItem }>();
</script>
{#if download.status === "completed"}
  <Badge variant="outline" title={download.upscale_model ?? $t("app.upscale.originalReady")}>
    {download.upscaled_at ? $t("app.upscale.scaled", { scale: download.upscale_scale ?? 2 }) : $t("app.upscale.original")}
  </Badge>
{/if}
