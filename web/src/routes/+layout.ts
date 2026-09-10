import { browser } from "$app/environment";
import { error, redirect } from "@sveltejs/kit";
import { LOCALE_COOKIE_NAME, getPreferredLocale, loadTranslations } from "$lib/i18n";
import { readSession, type BrowserSession } from "$lib/auth";
import { QUERY_CACHE_STORAGE_KEY, getBrowserAppQueryClient } from "$lib/query-client";
import type { LayoutLoad } from "./$types";

export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async ({ fetch, url, depends }) => {
  depends("app:session");
  const preferred = browser ? document.cookie.split(";").map((cookie) => cookie.trim().split("=")).find(([name]) => name === LOCALE_COOKIE_NAME)?.[1] : undefined;
  const locale = getPreferredLocale(preferred, browser ? navigator.language : undefined);
  await loadTranslations(locale, url.pathname);
  let session: BrowserSession = await readSession(fetch);
  const sharedLibrary = /^\/library\/([^/]+)$/.exec(url.pathname)?.[1];
  if (["1", "true"].includes(url.searchParams.get("public") ?? "") && sharedLibrary && !session.user) {
    const opened = await fetch(`/auth/public-share/${encodeURIComponent(sharedLibrary)}/open?json=true`);
    if (!opened.ok) error(opened.status, "This public share is unavailable.");
    session = await readSession(fetch);
  }
  const share = session.publicAccess;
  const sharedPath = share?.pathname;
  const sharedReader = url.pathname.startsWith("/read/") && sharedPath === `/library/${url.searchParams.get("libraryId")}`;
  const sharedRoute = sharedPath === url.pathname || sharedReader;
  if (browser) {
    const identity = session.user?.id ?? (session.authEnabled ? `share:${sharedPath ?? "none"}` : "public");
    const identityKey = `${QUERY_CACHE_STORAGE_KEY}:identity`;
    if (sessionStorage.getItem(identityKey) !== identity) {
      getBrowserAppQueryClient().clear();
      sessionStorage.removeItem(QUERY_CACHE_STORAGE_KEY);
      sessionStorage.setItem(identityKey, identity);
    }
  }
  if (session.authEnabled && !session.user && !sharedRoute && url.pathname !== "/login") redirect(307, `/login?next=${encodeURIComponent(url.pathname + url.search)}`);
  if ((!session.authEnabled || session.user) && url.pathname === "/login") redirect(307, "/");
  return {
    locale,
    auth: {
      enabled: session.authEnabled,
      authenticated: Boolean(session.user),
      oidcConfigured: session.oidcConfigured,
      user: session.user,
      publicAccess: !session.user && share ? { pathname: share.pathname, routeKey: share.routeKey, shareUrl: share.url } : null,
    },
  };
};
