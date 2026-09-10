<script lang="ts">
  import { Button } from "$lib/ui/button";
  import {
    getDownloadActionLabel,
    isCancelableDownloadStatus,
    isDownloadRemovalBusy,
  } from "$lib/features/downloads/status";
  import { t } from "$lib/i18n";
  import type { DownloadItem } from "$lib/types";

  const {
    download,
    removingIds,
    onRemove,
    stopPropagation = false,
    class: className = "",
  } = $props<{
    download: DownloadItem;
    removingIds: Set<string>;
    onRemove: (id: string) => void;
    stopPropagation?: boolean;
    class?: string;
  }>();

  const actionLabel = $derived(getDownloadActionLabel(download, removingIds, $t));
  const disabled = $derived(isDownloadRemovalBusy(download, removingIds));

  function handleClick(event: MouseEvent): void {
    if (stopPropagation) {
      event.stopPropagation();
    }

    onRemove(download.id);
  }
</script>

<Button
  variant={isCancelableDownloadStatus(download.status) ? "outline" : "destructive"}
  size="sm"
  class={className}
  onclick={handleClick}
  {disabled}
>
  {actionLabel}
</Button>
