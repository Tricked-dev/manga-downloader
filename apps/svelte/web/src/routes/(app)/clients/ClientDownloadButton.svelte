<script lang="ts">
  import { Download } from "@lucide/svelte";
  import { Button } from "$lib/ui/button";
  import { t } from "$lib/i18n";
  import type { ClientDownload } from "$lib/features/clients/clients";

  const {
    download,
    downloading,
    onDownload,
  }: {
    download: ClientDownload;
    downloading: boolean | undefined;
    onDownload: (download: ClientDownload) => void | Promise<void>;
  } = $props();

  function handleDownload() {
    void onDownload(download);
  }
</script>

<Button
  variant="outline"
  size="sm"
  class="gap-2 sm:shrink-0"
  disabled={downloading}
  onclick={handleDownload}
>
  <Download class="h-4 w-4" />
  {downloading ? $t("app.clients.building") : $t(download.labelKey)}
</Button>
