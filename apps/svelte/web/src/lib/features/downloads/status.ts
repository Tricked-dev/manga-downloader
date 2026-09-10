import type { BadgeVariant } from "$lib/ui/badge";
import type { DownloadItem } from "$lib/types";

type TranslateFn = (key: string, vars?: Record<string, unknown>) => string;

export type DownloadStatusFilter =
  | "all"
  | "active"
  | "queued"
  | "completed"
  | "failed"
  | "cancelled";

export type DownloadStatusCounts = {
  active: number;
  completed: number;
  failed: number;
  queued: number;
};

export const ACTIVE_DOWNLOAD_STATUSES = new Set([
  "downloading",
  "fetch",
  "conversion",
  "archive",
  "canceling",
]);
const CANCELABLE_DOWNLOAD_STATUSES = new Set(["queued", ...ACTIVE_DOWNLOAD_STATUSES]);

const DOWNLOAD_STATUS_METADATA: Record<
  string,
  {
    variant: BadgeVariant;
  }
> = {
  archive: {
    variant: "info",
  },
  canceling: {
    variant: "warning",
  },
  cancelled: {
    variant: "muted",
  },
  completed: {
    variant: "success",
  },
  conversion: {
    variant: "info",
  },
  downloading: {
    variant: "info",
  },
  error: {
    variant: "danger",
  },
  fetch: {
    variant: "info",
  },
  queued: {
    variant: "muted",
  },
};

const DOWNLOAD_STATUS_LABEL_KEYS: Record<string, { detail: string; stage: string }> = {
  archive: {
    detail: "app.downloads.statuses.archiveDetail",
    stage: "app.downloads.statuses.archiveStage",
  },
  canceling: {
    detail: "app.downloads.statuses.cancelingDetail",
    stage: "app.downloads.statuses.cancelingStage",
  },
  cancelled: {
    detail: "app.downloads.statuses.cancelledDetail",
    stage: "app.downloads.statuses.cancelledStage",
  },
  completed: {
    detail: "app.downloads.statuses.completedDetail",
    stage: "app.downloads.statuses.completedStage",
  },
  conversion: {
    detail: "app.downloads.statuses.conversionDetail",
    stage: "app.downloads.statuses.conversionStage",
  },
  downloading: {
    detail: "app.downloads.statuses.downloadingDetail",
    stage: "app.downloads.statuses.downloadingStage",
  },
  error: {
    detail: "app.downloads.statuses.errorDetail",
    stage: "app.downloads.statuses.errorStage",
  },
  fetch: {
    detail: "app.downloads.statuses.fetchDetail",
    stage: "app.downloads.statuses.fetchStage",
  },
  queued: {
    detail: "app.downloads.statuses.queuedDetail",
    stage: "app.downloads.statuses.queuedStage",
  },
};

export function isActiveDownloadStatus(status: string): boolean {
  return ACTIVE_DOWNLOAD_STATUSES.has(status);
}

export function isCancelableDownloadStatus(status: string): boolean {
  return CANCELABLE_DOWNLOAD_STATUSES.has(status);
}

export function isDownloadRemovalBusy(
  download: DownloadItem,
  removingIds: ReadonlySet<string>,
): boolean {
  return download.status === "canceling" || removingIds.has(download.id);
}

export function getDownloadStatusCounts(downloads: readonly DownloadItem[]): DownloadStatusCounts {
  return downloads.reduce<DownloadStatusCounts>(
    (counts, download) => {
      if (download.status === "completed") {
        counts.completed += 1;
      }
      if (ACTIVE_DOWNLOAD_STATUSES.has(download.status)) {
        counts.active += 1;
      }
      if (download.status === "queued") {
        counts.queued += 1;
      }
      if (download.status === "error") {
        counts.failed += 1;
      }

      return counts;
    },
    { active: 0, completed: 0, failed: 0, queued: 0 },
  );
}

export function countCancelableDownloads(downloads: readonly DownloadItem[]): number {
  let count = 0;
  for (const download of downloads) {
    if (isCancelableDownloadStatus(download.status)) {
      count += 1;
    }
  }

  return count;
}

export function hasBusyDownload(
  downloads: readonly DownloadItem[],
  removingIds: ReadonlySet<string>,
): boolean {
  return downloads.some((download) => removingIds.has(download.id));
}

export function matchesDownloadStatusFilter(
  download: DownloadItem,
  filter: DownloadStatusFilter,
): boolean {
  switch (filter) {
    case "active":
      return isActiveDownloadStatus(download.status);
    case "queued":
      return download.status === "queued";
    case "completed":
      return download.status === "completed";
    case "failed":
      return download.status === "error";
    case "cancelled":
      return download.status === "cancelled" || download.status === "canceling";
    default:
      return true;
  }
}

export function filterDownloadsByStatus(
  downloads: readonly DownloadItem[],
  filter: DownloadStatusFilter,
): DownloadItem[] {
  return downloads.filter((download) => matchesDownloadStatusFilter(download, filter));
}

export function getDownloadActionLabel(
  download: DownloadItem,
  removingIds: ReadonlySet<string>,
  translate: TranslateFn,
): string {
  if (download.status === "canceling") {
    return translate("app.actions.canceling");
  }

  if (removingIds.has(download.id)) {
    return isCancelableDownloadStatus(download.status)
      ? translate("app.actions.canceling")
      : translate("app.actions.deleting");
  }

  return isCancelableDownloadStatus(download.status)
    ? translate("app.actions.cancel")
    : translate("app.actions.delete");
}

export function getBulkDownloadActionLabel(
  selectedCount: number,
  selectedCancelableCount: number,
  busy: boolean,
  translate: TranslateFn,
): string {
  if (busy) {
    return translate("app.actions.updating");
  }

  if (selectedCount === 0) {
    return translate("app.actions.selectDownloads");
  }

  if (selectedCancelableCount === selectedCount) {
    return translate("app.actions.cancelSelected");
  }

  if (selectedCancelableCount === 0) {
    return translate("app.actions.deleteSelected");
  }

  return translate("app.actions.cancelDeleteSelected");
}

export function getDownloadStatusLabelKey(
  status: string,
  mode: "detail" | "stage",
): string | undefined {
  return DOWNLOAD_STATUS_LABEL_KEYS[status]?.[mode];
}

export function getDownloadDisplayLabel(status: string): string {
  const label = formatRawDownloadStatus(status);
  return label.charAt(0).toUpperCase() + label.slice(1);
}

export function getDownloadDetailLabel(status: string): string {
  return getDownloadDisplayLabel(status);
}

export function getTranslatedDownloadDisplayLabel(status: string, translate: TranslateFn): string {
  const key = getDownloadStatusLabelKey(status, "stage");
  return key ? translate(key) : getDownloadDisplayLabel(status);
}

export function getTranslatedDownloadDetailLabel(status: string, translate: TranslateFn): string {
  const key = getDownloadStatusLabelKey(status, "detail");
  return key ? translate(key) : getDownloadDetailLabel(status);
}

export function getDownloadStatusVariant(status: string): BadgeVariant {
  return DOWNLOAD_STATUS_METADATA[status]?.variant ?? "info";
}

function formatRawDownloadStatus(status: string): string {
  return status.trim().replace(/[-_]+/g, " ");
}
