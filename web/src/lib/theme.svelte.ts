import { browser } from "$app/environment";

export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

const THEME_STORAGE_KEY = "manga-dev-toolbar-theme";

class ThemeController {
  preference = $state<ThemePreference>("system");
  resolved = $state<ResolvedTheme>("dark");

  #initialized = false;
  #mediaQuery: MediaQueryList | undefined;

  initialize(): void {
    if (!browser || this.#initialized) {
      return;
    }

    this.#initialized = true;
    this.apply(this.#readStoredTheme(), false);
    this.#mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    this.#mediaQuery.addEventListener("change", this.#handleSystemThemeChange);
  }

  apply = (preference: ThemePreference, persist = true): void => {
    if (!browser) {
      return;
    }

    this.preference = preference;
    this.resolved = this.#resolvePreference(preference);
    document.documentElement.classList.toggle("dark", this.resolved === "dark");
    document.documentElement.style.colorScheme = this.resolved;

    if (persist) {
      localStorage.setItem(THEME_STORAGE_KEY, preference);
    }
  };

  toggle = (): void => {
    this.apply(this.resolved === "dark" ? "light" : "dark");
  };

  #handleSystemThemeChange = (): void => {
    if (this.preference === "system") {
      this.apply("system", false);
    }
  };

  #readStoredTheme(): ThemePreference {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    return stored === "light" || stored === "dark" || stored === "system" ? stored : "system";
  }

  #resolvePreference(preference: ThemePreference): ResolvedTheme {
    if (preference !== "system") {
      return preference;
    }

    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
}

export const themeController = new ThemeController();
