import { describe, expect, it } from "vitest";

import {
  buildChangedSettingsPayload,
  buildSettingsSaveSnapshot,
  DEFAULT_AVIF_CONVERSION_WORKERS,
  DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS,
  DEFAULT_DOWNLOAD_PAGE_FETCH_CONCURRENCY,
  encodeBinaryUnitSetting,
  getAvifConversionWorkers,
  getDownloadConcurrentChapters,
  getDownloadPageFetchConcurrency,
  getSourceAvifEnabled,
  getSourceAvifQuality,
  hasChangedSettings,
  parseBinaryUnitSetting,
  sourceAvifEnabledKey,
  sourceAvifQualityKey,
} from "./utils";

describe("settings utils", () => {
  it.each([
    [{ avif_conversion_workers: "8" }, 8],
    [{ avif_conversion_workers: "03" }, 3],
    [{ avif_conversion_workers: "0" }, DEFAULT_AVIF_CONVERSION_WORKERS],
    [{ avif_conversion_workers: "abc" }, DEFAULT_AVIF_CONVERSION_WORKERS],
    [{}, DEFAULT_AVIF_CONVERSION_WORKERS],
  ])("parses AVIF worker counts from %o", (settings, expected) => {
    expect(getAvifConversionWorkers(settings)).toBe(expected);
  });

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

  it("reads source AVIF settings with sensible defaults", () => {
    expect(getSourceAvifEnabled({ [sourceAvifEnabledKey("example")]: "true" }, "example")).toBe(
      true,
    );
    expect(getSourceAvifEnabled({}, "example")).toBe(false);

    expect(getSourceAvifQuality({ [sourceAvifQualityKey("example")]: "92" }, "example")).toBe(92);
    expect(getSourceAvifQuality({ [sourceAvifQualityKey("example")]: "bad" }, "example")).toBe(80);
  });

  it("keeps source settings isolated by source name", () => {
    const settings = {
      [sourceAvifEnabledKey("example")]: "true",
      [sourceAvifQualityKey("example")]: "92",
      [sourceAvifEnabledKey("other")]: "false",
      [sourceAvifQualityKey("other")]: "67",
    };

    expect(getSourceAvifEnabled(settings, "example")).toBe(true);
    expect(getSourceAvifQuality(settings, "example")).toBe(92);
    expect(getSourceAvifEnabled(settings, "other")).toBe(false);
    expect(getSourceAvifQuality(settings, "other")).toBe(67);
  });

  it("normalizes save snapshots before comparing or persisting settings", () => {
    const snapshot = buildSettingsSaveSnapshot(
      {
        avif_conversion_workers: "0",
        download_concurrent_chapters: "0",
        download_page_fetch_concurrency: "40",
        library_categories: " default, downloaded, ",
        update_interval_hours: "",
        [sourceAvifEnabledKey("example")]: "true",
      },
      ["example"],
    );

    expect(snapshot.avif_conversion_workers).toBe(`${DEFAULT_AVIF_CONVERSION_WORKERS}`);
    expect(snapshot.download_concurrent_chapters).toBe(`${DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS}`);
    expect(snapshot.download_page_fetch_concurrency).toBe("32");
    expect(snapshot.library_categories).toBe("default,downloaded");
    expect(snapshot.update_interval_hours).toBe("1");
    expect(snapshot[sourceAvifEnabledKey("example")]).toBe("true");
    expect(snapshot[sourceAvifQualityKey("example")]).toBe("80");
  });

  it("builds a settings payload with only changed keys", () => {
    const previous = buildSettingsSaveSnapshot(
      {
        download_path: "/old",
        [sourceAvifQualityKey("example")]: "80",
      },
      ["example"],
    );
    const next = buildSettingsSaveSnapshot(
      {
        download_path: "/new",
        [sourceAvifQualityKey("example")]: "80",
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
