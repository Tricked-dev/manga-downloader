<script lang="ts">
  import { Badge } from "$lib/ui/badge";
  import { t } from "$lib/i18n";
  import { Check, Clock3, Download, OctagonAlert, TimerReset } from "@lucide/svelte";

  const {
    total,
    active,
    queued,
    completed,
    failed,
  } = $props<{
    total: number;
    active: number;
    queued: number;
    completed: number;
    failed: number;
  }>();

</script>

<div class="flex flex-wrap gap-2">
  <Badge variant="outline" class="h-9 gap-2 rounded-lg px-3 text-sm font-medium">
    <Download class="h-3.5 w-3.5 text-primary" />
    <span>{total}</span>
    <span class="text-muted-foreground">{$t("app.downloads.summary.total")}</span>
  </Badge>
  <Badge variant="outline" class="h-9 gap-2 rounded-lg px-3 text-sm font-medium">
    <TimerReset class="h-3.5 w-3.5 text-sky-400" />
    <span>{active}</span>
    <span class="text-muted-foreground">{$t("app.downloads.summary.active")}</span>
  </Badge>
  <Badge variant="outline" class="h-9 gap-2 rounded-lg px-3 text-sm font-medium">
    <Clock3 class="h-3.5 w-3.5 text-muted-foreground" />
    <span>{queued}</span>
    <span class="text-muted-foreground">{$t("app.downloads.summary.queued")}</span>
  </Badge>
  <Badge variant="outline" class="h-9 gap-2 rounded-lg px-3 text-sm font-medium">
    <Check class="h-3.5 w-3.5 text-green-500" />
    <span>{completed}</span>
    <span class="text-muted-foreground">{$t("app.downloads.summary.completed")}</span>
  </Badge>
  <Badge variant="outline" class="h-9 gap-2 rounded-lg px-3 text-sm font-medium">
    <OctagonAlert class="h-3.5 w-3.5 text-destructive" />
    <span>{failed}</span>
    <span class="text-muted-foreground">{$t("app.downloads.summary.failed")}</span>
  </Badge>
</div>
