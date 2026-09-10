<script lang="ts">
  import { Badge } from "$lib/ui/badge";
  import { t } from "$lib/i18n";

  let {
    downloading,
    queued,
  } = $props<{
    downloading: boolean;
    queued: boolean;
  }>();
</script>

{#if downloading}
  <Badge variant="secondary">{$t("app.manga.downloading")}</Badge>
{:else if queued}
  <Badge variant="success">{$t("app.manga.queued")}</Badge>
{:else}
  <Badge variant="outline">{$t("app.manga.available")}</Badge>
{/if}
