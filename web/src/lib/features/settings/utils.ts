import type { SettingsMap } from "./types";
import { DEFAULT_SETTINGS } from "$lib/settings";
import { csvFromList } from "$lib/utils";

export const DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS = 2;
export const DEFAULT_DOWNLOAD_PAGE_FETCH_CONCURRENCY = 2;
const MAX_CONCURRENCY_SETTING = 32;
const BYTES_PER_MIB = 1024 * 1024;
const BYTES_PER_GIB = 1024 * BYTES_PER_MIB;

export type BinaryUnit = "MiB" | "GiB";

export interface BinaryUnitSetting {
  amount: string;
  enabled: boolean;
  unit: BinaryUnit;
}

export function parseBinaryUnitSetting(
  value: string | undefined,
  {
    defaultAmount,
    defaultUnit,
    enabledFallback = true,
  }: {
    defaultAmount: string;
    defaultUnit: BinaryUnit;
    enabledFallback?: boolean;
  },
): BinaryUnitSetting {
  const normalized = (value ?? "").trim();
  if (!normalized) {
    return { amount: defaultAmount, enabled: enabledFallback, unit: defaultUnit };
  }

  const lowered = normalized.toLowerCase();
  if (lowered.endsWith("gib")) {
    const amount = Number.parseInt(normalized.slice(0, -3).trim(), 10);
    return {
      amount: String(Number.isFinite(amount) && amount > 0 ? amount : 1),
      enabled: true,
      unit: "GiB",
    };
  }

  if (lowered.endsWith("mib")) {
    const amount = Number.parseInt(normalized.slice(0, -3).trim(), 10);
    return {
      amount: String(Number.isFinite(amount) && amount > 0 ? amount : 1),
      enabled: true,
      unit: "MiB",
    };
  }

  const bytes = Number.parseInt(normalized, 10);
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return { amount: defaultAmount, enabled: enabledFallback, unit: defaultUnit };
  }

  if (bytes % BYTES_PER_GIB === 0) {
    return {
      amount: String(Math.max(1, Math.round(bytes / BYTES_PER_GIB))),
      enabled: true,
      unit: "GiB",
    };
  }

  return {
    amount: String(Math.max(1, Math.round(bytes / BYTES_PER_MIB))),
    enabled: true,
    unit: "MiB",
  };
}

export function encodeBinaryUnitSetting(amountValue: string, unit: BinaryUnit): string {
  const amount = Number.parseInt(amountValue, 10);
  return `${Number.isFinite(amount) && amount > 0 ? amount : 1}${unit}`;
}

export function getDownloadConcurrentChapters(settings: SettingsMap): number {
  return getPositiveIntegerSetting(
    settings.download_concurrent_chapters,
    DEFAULT_DOWNLOAD_CONCURRENT_CHAPTERS,
    MAX_CONCURRENCY_SETTING,
  );
}

export function getDownloadPageFetchConcurrency(settings: SettingsMap): number {
  return getPositiveIntegerSetting(
    settings.download_page_fetch_concurrency,
    DEFAULT_DOWNLOAD_PAGE_FETCH_CONCURRENCY,
    MAX_CONCURRENCY_SETTING,
  );
}

export function buildSettingsSaveSnapshot(
  settings: SettingsMap,
  sourceNames: string[],
): SettingsMap {
  const settingsSnapshot = { ...DEFAULT_SETTINGS, ...settings };
  const payload: SettingsMap = {
    ...settingsSnapshot,
    auto_upscale: settingsSnapshot.auto_upscale ?? "true",
    download_concurrent_chapters: `${getDownloadConcurrentChapters(settingsSnapshot)}`,
    download_page_fetch_concurrency: `${getDownloadPageFetchConcurrency(settingsSnapshot)}`,
    library_categories: csvFromList(settingsSnapshot.library_categories),
    update_interval_hours: settingsSnapshot.update_interval_hours || "1",
  };

  for (const sourceName of sourceNames) {
    payload[`source.${sourceName}.auto_upscale`] = settingsSnapshot[`source.${sourceName}.auto_upscale`] ?? "true";
  }

  return payload;
}

export function buildChangedSettingsPayload(
  nextSettings: SettingsMap,
  previousSettings: SettingsMap,
): SettingsMap {
  return Object.fromEntries(
    Object.entries(nextSettings).filter(([key, value]) => previousSettings[key] !== value),
  );
}

export function hasChangedSettings(settings: SettingsMap): boolean {
  return Object.keys(settings).length > 0;
}

function getPositiveIntegerSetting(
  value: string | undefined,
  defaultValue: number,
  maxValue: number,
): number {
  const parsed = Number.parseInt(value ?? `${defaultValue}`, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return defaultValue;
  }
  return Math.min(Math.floor(parsed), maxValue);
}
