<script lang="ts">
  import { Badge } from "$lib/ui/badge";
  import {
    getDownloadStatusVariant,
    getTranslatedDownloadDetailLabel,
    getTranslatedDownloadDisplayLabel,
  } from "$lib/features/downloads/status";
  import { t } from "$lib/i18n";

  const {
    status,
    mode = "stage",
    class: className,
  } = $props<{
    status: string;
    mode?: "detail" | "stage";
    class?: string;
  }>();

  const label = $derived(mode === "detail" ? getTranslatedDownloadDetailLabel(status, $t) : getTranslatedDownloadDisplayLabel(status, $t));
  const variant = $derived(getDownloadStatusVariant(status));
</script>

<Badge {variant} class={className}>
  {label}
</Badge>
