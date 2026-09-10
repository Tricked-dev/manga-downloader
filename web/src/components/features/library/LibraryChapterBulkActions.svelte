<script lang="ts">
  import { Button } from "$lib/ui/button";
  import { t } from "$lib/i18n";
  import { BookOpenCheck, BookX, CircleArrowDown, FileCog, Trash2, X } from "@lucide/svelte";

  let {
    clearActionDisabled,
    deleteActionDisabled,
    downloadActionDisabled,
    upscaleActionDisabled,
    markReadActionDisabled,
    markUnreadActionDisabled,
    selectedCount,
    selectedDeleteLoading,
    selectedReadLoading,
    selectedUpscaleLoading,
    selectedUnreadLoading,
    bulkDownloadLoading,
    disabledActionClass,
    onClear,
    onDelete,
    onDownload,
    onMarkRead,
    onMarkUnread,
    onUpscale,
  } = $props<{
    clearActionDisabled: boolean;
    deleteActionDisabled: boolean;
    downloadActionDisabled: boolean;
    upscaleActionDisabled: boolean;
    markReadActionDisabled: boolean;
    markUnreadActionDisabled: boolean;
    selectedCount: number;
    selectedDeleteLoading: boolean;
    selectedReadLoading: boolean;
    selectedUpscaleLoading: boolean;
    selectedUnreadLoading: boolean;
    bulkDownloadLoading: boolean;
    disabledActionClass: (disabled: boolean) => string;
    onClear: (event: MouseEvent) => void;
    onDelete: (event: MouseEvent) => void;
    onDownload: (event: MouseEvent) => void;
    onMarkRead: (event: MouseEvent) => void;
    onMarkUnread: (event: MouseEvent) => void;
    onUpscale: (event: MouseEvent) => void;
  }>();

  const actions = $derived(getActions());

  function getActions() {
    return [
      {
        ariaLabel: bulkDownloadLoading ? $t("app.libraryDetail.queueingSelected") : $t("app.libraryDetail.downloadSelected"),
        className: disabledActionClass(downloadActionDisabled),
        description: $t("app.libraryDetail.downloadSelectedDescription"),
        disabled: downloadActionDisabled,
        icon: CircleArrowDown,
        label: $t("app.libraryDetail.downloadSelected"),
        onclick: onDownload,
        variant: "outline" as const,
      },
      {
        ariaLabel: $t("app.libraryDetail.upscaleSelected"),
        className: disabledActionClass(upscaleActionDisabled),
        description: $t("app.libraryDetail.upscaleSelectedDescription"),
        disabled: upscaleActionDisabled,
        icon: FileCog,
        iconClass: selectedUpscaleLoading ? "animate-pulse" : "",
        label: $t("app.libraryDetail.upscaleSelected"),
        onclick: onUpscale,
        variant: "outline" as const,
      },
      {
        ariaLabel: $t("app.libraryDetail.markSelectedRead"),
        className: disabledActionClass(markReadActionDisabled),
        description: $t("app.libraryDetail.markReadDescription"),
        disabled: markReadActionDisabled,
        icon: BookOpenCheck,
        iconClass: selectedReadLoading ? "animate-pulse" : "",
        label: $t("app.libraryDetail.markSelectedRead"),
        onclick: onMarkRead,
        variant: "outline" as const,
      },
      {
        ariaLabel: $t("app.libraryDetail.markSelectedUnread"),
        className: disabledActionClass(markUnreadActionDisabled),
        description: $t("app.libraryDetail.markUnreadDescription"),
        disabled: markUnreadActionDisabled,
        icon: BookX,
        iconClass: selectedUnreadLoading ? "animate-pulse" : "",
        label: $t("app.libraryDetail.markSelectedUnread"),
        onclick: onMarkUnread,
        variant: "outline" as const,
      },
      {
        ariaLabel: $t("app.actions.deleteSelected"),
        className: disabledActionClass(deleteActionDisabled),
        description: $t("app.libraryDetail.deleteSelectedDescription"),
        disabled: deleteActionDisabled,
        icon: Trash2,
        iconClass: selectedDeleteLoading ? "animate-pulse" : "",
        label: $t("app.actions.deleteSelected"),
        onclick: onDelete,
        variant: "destructive" as const,
      },
      {
        ariaLabel: $t("app.actions.clearSelection"),
        className: disabledActionClass(clearActionDisabled),
        description: $t("app.libraryDetail.clearSelectionDescription"),
        disabled: clearActionDisabled,
        icon: X,
        label: $t("app.actions.clearSelection"),
        onclick: onClear,
        variant: "outline" as const,
      },
    ];
  }
</script>

<div class="border-b border-border/70 bg-card px-4 py-3">
  <div class="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
    <p class="text-xs text-muted-foreground">
      {#if selectedCount === 0}
        {$t("app.libraryDetail.selectChaptersBulk")}
      {:else}
        {$t("app.libraryDetail.selected", { count: selectedCount })}
      {/if}
    </p>
    <div class="flex shrink-0 flex-wrap items-center justify-end gap-2">
      {#each actions as action (action.label)}
        <div class="group/action relative">
          <Button
            variant={action.variant}
            size="icon-sm"
            class={action.className}
            aria-label={action.ariaLabel}
            aria-disabled={action.disabled}
            onclick={action.onclick}
          >
            <action.icon class={`h-3.5 w-3.5 ${action.iconClass ?? ""}`} />
          </Button>
          <div class="pointer-events-none absolute bottom-full right-0 z-50 mb-2 w-64 translate-y-1 rounded-lg bg-popover p-2.5 text-left text-xs leading-5 text-popover-foreground opacity-0 shadow-md ring-1 ring-foreground/10 transition group-hover/action:translate-y-0 group-hover/action:opacity-100 group-focus-within/action:translate-y-0 group-focus-within/action:opacity-100">
            <p class="font-medium text-foreground">{action.label}</p>
            <p class="text-muted-foreground">{action.description}</p>
          </div>
        </div>
      {/each}
    </div>
  </div>
</div>
