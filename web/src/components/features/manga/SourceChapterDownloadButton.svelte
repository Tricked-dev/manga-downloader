<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { t } from "$lib/i18n";
  import { Check, DownloadCloud, Loader2 } from "@lucide/svelte";

  let {
    chapterId,
    downloading,
    queued,
    onDownload,
  } = $props<{
    chapterId: string;
    downloading: boolean;
    queued: boolean;
    onDownload: (chapterId: string) => void;
  }>();

  function handleDownload() {
    onDownload(chapterId);
  }
</script>

<Button
  variant={queued ? "secondary" : "outline"}
  size="sm"
  onclick={handleDownload}
  disabled={downloading || queued}
>
  {#if downloading}
    <Loader2 class="mr-2 h-4 w-4 animate-spin" /> {$t("app.manga.downloadingProgress")}
  {:else if queued}
    <Check class="mr-2 h-4 w-4" /> {$t("app.manga.queued")}
  {:else}
    <DownloadCloud class="mr-2 h-4 w-4" /> {$t("app.manga.download")}
  {/if}
</Button>
