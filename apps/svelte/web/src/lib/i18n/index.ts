import { browser } from "$app/environment";
import i18n, { type Config } from "sveltekit-i18n";

type TranslationPayload = Record<string, unknown>;

const DEFAULT_LOCALE = "en";
export const LOCALE_COOKIE_NAME = "manga_locale";
const LOCALE_STORAGE_KEY = "manga.locale";

const supportedLocales = ["en", "de", "es", "fr", "it", "pl"] as const;
export type AppLocale = (typeof supportedLocales)[number];

export const localeOptions: Array<{ value: AppLocale; labelKey: string }> = [
  { value: "en", labelKey: "app.locale.options.en" },
  { value: "de", labelKey: "app.locale.options.de" },
  { value: "es", labelKey: "app.locale.options.es" },
  { value: "fr", labelKey: "app.locale.options.fr" },
  { value: "it", labelKey: "app.locale.options.it" },
  { value: "pl", labelKey: "app.locale.options.pl" },
];

const config: Config<TranslationPayload> = {
  fallbackLocale: DEFAULT_LOCALE,
  loaders: [
    {
      locale: "en",
      key: "app",
      loader: async () => (await import("./locales/en/app.json")).default,
    },
    {
      locale: "de",
      key: "app",
      loader: async () => (await import("./locales/de/app.json")).default,
    },
    {
      locale: "es",
      key: "app",
      loader: async () => (await import("./locales/es/app.json")).default,
    },
    {
      locale: "fr",
      key: "app",
      loader: async () => (await import("./locales/fr/app.json")).default,
    },
    {
      locale: "it",
      key: "app",
      loader: async () => (await import("./locales/it/app.json")).default,
    },
    {
      locale: "pl",
      key: "app",
      loader: async () => (await import("./locales/pl/app.json")).default,
    },
  ],
  log: {
    level: "error",
  },
};

const instance = new i18n(config);

export const { t, locale, loadTranslations } = instance;

const { setLocale } = instance;

function isSupportedLocale(value: string | null | undefined): value is AppLocale {
  return supportedLocales.includes(value as AppLocale);
}

export function getPreferredLocale(...candidates: Array<string | null | undefined>): AppLocale {
  for (const candidate of candidates) {
    const locale = parseLocaleCandidate(candidate);
    if (locale) {
      return locale;
    }
  }

  return DEFAULT_LOCALE;
}

export async function switchLocale(nextLocale: string): Promise<void> {
  const preferredLocale = getPreferredLocale(nextLocale);
  persistLocale(preferredLocale);
  await setLocale(preferredLocale);
}

function persistLocale(nextLocale: AppLocale) {
  if (!browser) {
    return;
  }

  localStorage.setItem(LOCALE_STORAGE_KEY, nextLocale);
  document.cookie = `${LOCALE_COOKIE_NAME}=${nextLocale}; path=/; max-age=31536000; SameSite=Lax`;
  document.documentElement.lang = nextLocale;
}

function parseLocaleCandidate(candidate: string | null | undefined): AppLocale | undefined {
  if (!candidate) {
    return undefined;
  }

  for (const value of candidate.split(",")) {
    const locale = value.trim().split(";")[0]?.toLowerCase().split("-")[0];
    if (isSupportedLocale(locale)) {
      return locale;
    }
  }

  return undefined;
}
