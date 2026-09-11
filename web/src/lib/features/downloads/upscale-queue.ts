import type { BadgeVariant } from "$lib/ui/badge";
import type { UpscaleQueueEntry } from "@manga-server/api-client/generated";

const UPSCALE_STATUS_VARIANTS: Record<string, BadgeVariant> = {
  failed: "danger",
  paused: "warning",
  queued: "muted",
  running: "info",
};

export function upscaleStatusVariant(status: string): BadgeVariant {
  return UPSCALE_STATUS_VARIANTS[status] ?? "outline";
}

/** A job reports pages only once it has inspected the chapter, so an unknown total is 0. */
export function upscalePercent(entry: UpscaleQueueEntry): number {
  if (entry.total_pages <= 0) {
    return 0;
  }
  const ratio = (entry.completed_pages / entry.total_pages) * 100;
  return Math.min(100, Math.max(0, Math.round(ratio)));
}

export function countUpscaleStatus(entries: UpscaleQueueEntry[], status: string): number {
  return entries.filter((entry) => entry.status === status).length;
}

/** Running work first, then what is waiting, then what needs attention. */
const UPSCALE_STATUS_ORDER = ["running", "paused", "queued", "failed"];

export function sortUpscaleQueue(entries: UpscaleQueueEntry[]): UpscaleQueueEntry[] {
  return [...entries].sort((left, right) => {
    const byStatus =
      UPSCALE_STATUS_ORDER.indexOf(left.status) - UPSCALE_STATUS_ORDER.indexOf(right.status);
    if (byStatus !== 0) {
      return byStatus;
    }
    return right.updated_at.localeCompare(left.updated_at);
  });
}
