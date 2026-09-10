import type { SettingsMap } from "$lib/features/settings/types";

export function selectItems<T>(response: { items: T[] }): T[] {
  return response.items;
}

export function selectSettings(response: { settings: SettingsMap }): SettingsMap {
  return response.settings;
}
