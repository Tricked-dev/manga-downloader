<script lang="ts">
  import { Search } from "@lucide/svelte";
  import { Input } from "$lib/ui/input";
  import { Select, SelectContent, SelectItem, SelectTrigger } from "$lib/ui/select";
  import { t } from "$lib/i18n";

  type StatusFilter = "all" | "active" | "queued" | "completed" | "failed" | "cancelled";
  type SortOption = "queue" | "series-asc" | "series-desc";

  const STATUS_FILTER_LABEL_KEYS: Record<StatusFilter, string> = {
    active: "app.downloads.active",
    all: "app.downloads.allStatuses",
    cancelled: "app.downloads.cancelled",
    completed: "app.downloads.completed",
    failed: "app.downloads.failed",
    queued: "app.downloads.queued",
  };
  const SORT_OPTION_LABEL_KEYS: Record<SortOption, string> = {
    queue: "app.downloads.queueOrder",
    "series-asc": "app.downloads.seriesAsc",
    "series-desc": "app.downloads.seriesDesc",
  };

  const {
    globalFilter,
    onGlobalFilterInput,
    setSortOption,
    setStatusFilter,
    sortOption,
    statusFilter,
  }: {
    globalFilter: string;
    onGlobalFilterInput: (event: Event) => void;
    setSortOption: (value: string) => void;
    setStatusFilter: (value: string) => void;
    sortOption: SortOption;
    statusFilter: StatusFilter;
  } = $props();
</script>

<div class="relative">
  <Search class="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
  <Input
    placeholder={$t("app.downloads.search")}
    value={globalFilter}
    oninput={onGlobalFilterInput}
    aria-label={$t("app.downloads.search")}
    class="h-9 pl-9"
  />
</div>
<Select type="single" value={statusFilter} onValueChange={setStatusFilter}>
  <SelectTrigger class="h-9 w-full" aria-label={$t("app.downloads.filterByStatus")}>
    {$t(STATUS_FILTER_LABEL_KEYS[statusFilter])}
  </SelectTrigger>
  <SelectContent>
    <SelectItem value="all">{$t("app.downloads.allStatuses")}</SelectItem>
    <SelectItem value="active">{$t("app.downloads.active")}</SelectItem>
    <SelectItem value="queued">{$t("app.downloads.queued")}</SelectItem>
    <SelectItem value="completed">{$t("app.downloads.completed")}</SelectItem>
    <SelectItem value="failed">{$t("app.downloads.failed")}</SelectItem>
    <SelectItem value="cancelled">{$t("app.downloads.cancelled")}</SelectItem>
  </SelectContent>
</Select>
<Select type="single" value={sortOption} onValueChange={setSortOption}>
  <SelectTrigger class="h-9 w-full" aria-label={$t("app.downloads.sort")}>
    {$t(SORT_OPTION_LABEL_KEYS[sortOption])}
  </SelectTrigger>
  <SelectContent>
    <SelectItem value="queue">{$t("app.downloads.queueOrder")}</SelectItem>
    <SelectItem value="series-asc">{$t("app.downloads.seriesAsc")}</SelectItem>
    <SelectItem value="series-desc">{$t("app.downloads.seriesDesc")}</SelectItem>
  </SelectContent>
</Select>
