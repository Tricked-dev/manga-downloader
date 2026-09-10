import { describe, expect, it } from "vitest";

import {
  buildChangedSettingsPayload,
  buildSettingsSaveSnapshot,
  DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS,
  DEFAULT_DOWNLOAD_PAGE_FETCH_CONCURRENCY,
  encodeBinaryUnitSetting,
  getDownloadConcurrentChapters,
  getDownloadPageFetchConcurrency,
  hasChangedSettings,
  parseBinaryUnitSetting,
} from "./utils";

describe("settings utils", () => {
  it.each([
    [{ download_concurrent_chapters: "4" }, 4],
    [{ download_concurrent_chapters: "0" }, DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS],
    [{ download_concurrent_chapters: "40" }, 32],
    [{}, DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS],
  ])("parses concurrent chapter download counts from %o", (settings, expected) => {
    expect(getDownloadConcurrentChapters(settings)).toBe(expected);
  });

  it.each([
    [{ download_page_fetch_concurrency: "6" }, 6],
    [{ download_page_fetch_concurrency: "0" }, DEFAULT_DOWNLOAD_PAGE_FETCH_CONCURRENCY],
    [{ download_page_fetch_concurrency: "40" }, 32],
    [{}, DEFAULT_DOWNLOAD_PAGE_FETCH_CONCURRENCY],
  ])("parses page fetch concurrency counts from %o", (settings, expected) => {
    expect(getDownloadPageFetchConcurrency(settings)).toBe(expected);
  });

  it("keeps automatic upscaling enabled by default and respects each source override", () => {
    const snapshot = buildSettingsSaveSnapshot({ "source.example.auto_upscale": "false" }, ["example", "other"]);
    expect(snapshot.auto_upscale).toBe("true");
    expect(snapshot["source.example.auto_upscale"]).toBe("false");
    expect(snapshot["source.other.auto_upscale"]).toBe("true");
  });

  it("normalizes save snapshots before comparing or persisting settings", () => {
    const snapshot = buildSettingsSaveSnapshot(
      {
        download_concurrent_chapters: "0",
        download_page_fetch_concurrency: "40",
        library_categories: " default, downloaded, ",
        update_interval_hours: "",
        "source.example.auto_upscale": "true",
      },
      ["example"],
    );

    expect(snapshot.auto_upscale).toBe("true");
    expect(snapshot.download_concurrent_chapters).toBe(`${DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS}`);
    expect(snapshot.download_page_fetch_concurrency).toBe("32");
    expect(snapshot.library_categories).toBe("default,downloaded");
    expect(snapshot.update_interval_hours).toBe("1");
    expect(snapshot["source.example.auto_upscale"]).toBe("true");
  });

  it("builds a settings payload with only changed keys", () => {
    const previous = buildSettingsSaveSnapshot(
      {
        download_path: "/old",
        "source.example.auto_upscale": "true",
      },
      ["example"],
    );
    const next = buildSettingsSaveSnapshot(
      {
        download_path: "/new",
        "source.example.auto_upscale": "true",
      },
      ["example"],
    );

    expect(buildChangedSettingsPayload(next, previous)).toEqual({
      download_path: "/new",
    });
  });

  it("detects whether a changed settings payload has entries", () => {
    expect(hasChangedSettings({ download_path: "/new" })).toBe(true);
    expect(hasChangedSettings({})).toBe(false);
  });

  it.each([
    ["2GiB", { amount: "2", enabled: true, unit: "GiB" }],
    ["512MiB", { amount: "512", enabled: true, unit: "MiB" }],
    [`${1024 * 1024 * 1024}`, { amount: "1", enabled: true, unit: "GiB" }],
    ["", { amount: "25", enabled: false, unit: "GiB" }],
    ["0", { amount: "25", enabled: false, unit: "GiB" }],
  ])("parses binary unit settings from %s", (value, expected) => {
    expect(
      parseBinaryUnitSetting(value, {
        defaultAmount: "25",
        defaultUnit: "GiB",
        enabledFallback: false,
      }),
    ).toEqual(expected);
  });

  it("encodes binary unit settings with a positive amount fallback", () => {
    expect(encodeBinaryUnitSetting("4", "GiB")).toBe("4GiB");
    expect(encodeBinaryUnitSetting("0", "MiB")).toBe("1MiB");
  });
});
