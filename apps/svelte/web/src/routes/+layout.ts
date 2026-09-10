import { getPreferredLocale, loadTranslations } from "$lib/i18n";
import type { LayoutLoad } from "./$types";

export const load = (async ({ data, url }) => {
  const locale = getPreferredLocale(data.locale);

  await loadTranslations(locale, url.pathname);

  return {
    ...data,
    locale,
  };
}) satisfies LayoutLoad;
