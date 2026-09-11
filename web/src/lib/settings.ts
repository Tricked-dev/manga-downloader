import type { SettingsMap } from "$lib/features/settings/types";

const DEFAULT_LIBRARY_CATEGORIES = ["default", "downloaded"] as const;

export const DEFAULT_SETTINGS: SettingsMap = {
  auth_enabled: "false",
  auth_oidc_client_id: "",
  auth_oidc_client_secret: "",
  auth_oidc_issuer_url: "",
  auth_oidc_provider_id: "oidc",
  auth_oidc_scopes: "openid profile email",
  auto_download_category: "",
  auto_download_new_chapters: "false",
  auto_upscale: "true",
  backend_api_key: "",
  cache_disk_path: "./data/cache",
  cache_max_memory_bytes: "268435456",
  download_concurrent_chapters: "2",
  download_page_fetch_concurrency: "2",
  download_path: "./data/downloads",
  library_categories: DEFAULT_LIBRARY_CATEGORIES.join(","),
  max_download_storage_bytes: "",
  update_interval_hours: "1",
  upscale_auto_resume_minutes: "0",
};

export const UPDATE_INTERVAL_OPTIONS = [
  { labelKey: "app.settings.intervals.thirtyMinutes", value: "0.5" },
  { labelKey: "app.settings.intervals.oneHour", value: "1" },
  { labelKey: "app.settings.intervals.sixHours", value: "6" },
  { labelKey: "app.settings.intervals.twelveHours", value: "12" },
  { labelKey: "app.settings.intervals.twentyFourHours", value: "24" },
  { labelKey: "app.settings.intervals.weekly", value: "168" },
];
