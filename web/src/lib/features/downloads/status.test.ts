import { describe, expect, it } from "vitest";

import type { DownloadItem } from "$lib/types";
import {
  getDownloadDetailLabel,
  getDownloadDisplayLabel,
  getDownloadStatusLabelKey,
  getDownloadStatusCounts,
  getDownloadStatusVariant,
  getTranslatedDownloadDetailLabel,
  getTranslatedDownloadDisplayLabel,
  isActiveDownloadStatus,
  isCancelableDownloadStatus,
} from "./status";

function download(status: string): DownloadItem {
  return {
    chapter_id: `${status}-chapter`,
    chapter_number: 1,
    chapter_source_id: `${status}-remote-chapter`,
    chapter_title: "Chapter 1",
    error: null,
    id: `${status}-download`,
    manga_id: `${status}-manga`,
    manga_source: "Comix",
    manga_source_id: `${status}-remote-manga`,
    manga_title: "Series",
    progress: 0,
    status,
  };
}

describe("download status helpers", () => {
  it("maps known statuses to translation keys", () => {
    expect(getDownloadStatusLabelKey("fetch", "stage")).toBe("app.downloads.statuses.fetchStage");
    expect(getDownloadStatusLabelKey("fetch", "detail")).toBe("app.downloads.statuses.fetchDetail");
  });

  it("translates known statuses with the provided translator", () => {
    const translate = (key: string) => `translated:${key}`;

    expect(getTranslatedDownloadDisplayLabel("conversion", translate)).toBe(
      "translated:app.downloads.statuses.conversionStage",
    );
    expect(getTranslatedDownloadDetailLabel("conversion", translate)).toBe(
      "translated:app.downloads.statuses.conversionDetail",
    );
  });

  it("formats unknown statuses without requiring a translation key", () => {
    expect(getDownloadStatusLabelKey("post_process", "stage")).toBeUndefined();
    expect(getDownloadDisplayLabel("post_process")).toBe("Post process");
    expect(getDownloadDetailLabel("post_process")).toBe("Post process");
  });

  it.each([
    ["completed", "success"],
    ["canceling", "warning"],
    ["cancelled", "muted"],
    ["error", "danger"],
    ["queued", "muted"],
    ["downloading", "info"],
    ["unknown", "info"],
  ])("returns the expected badge variant for %s", (status, expectedVariant) => {
    expect(getDownloadStatusVariant(status)).toBe(expectedVariant);
  });

  it.each(["downloading", "fetch", "conversion", "archive", "canceling"])(
    "marks %s as active and cancelable",
    (status) => {
      expect(isActiveDownloadStatus(status)).toBe(true);
      expect(isCancelableDownloadStatus(status)).toBe(true);
    },
  );

  it("allows queued downloads to be canceled without marking them active", () => {
    expect(isActiveDownloadStatus("queued")).toBe(false);
    expect(isCancelableDownloadStatus("queued")).toBe(true);
  });

  it.each(["completed", "cancelled", "error", "unknown"])(
    "does not allow terminal or unknown status %s to be canceled",
    (status) => {
      expect(isActiveDownloadStatus(status)).toBe(false);
      expect(isCancelableDownloadStatus(status)).toBe(false);
    },
  );

  it("counts download status buckets", () => {
    expect(
      getDownloadStatusCounts([
        download("queued"),
        download("fetch"),
        download("completed"),
        download("error"),
      ]),
    ).toEqual({ active: 1, completed: 1, failed: 1, queued: 1 });
  });
});
