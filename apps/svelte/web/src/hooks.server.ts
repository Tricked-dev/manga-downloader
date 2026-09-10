import { building } from "$app/environment";
import { env as publicEnv } from "$env/dynamic/public";
import { configureApiClient } from "@manga-server/api-client/base";
import { LOCALE_COOKIE_NAME, getPreferredLocale } from "$lib/i18n";
import { ensureAuthStorageReady, getAuthContext } from "$lib/server/auth/config";
import { getBackendBaseUrl } from "$lib/server/api/backend-config";
import { isPublicPath } from "$lib/server/auth/session";
import {
  PUBLIC_SHARE_COOKIE_MAX_AGE,
  PUBLIC_SHARE_COOKIE_NAME,
  isPublicShareApiRequestAllowed,
  isPublicShareRequestAllowed,
  publicShareUrlForRecord,
  resolvePublicShareAccess,
} from "$lib/server/auth/public-share";
import { error, redirect } from "@sveltejs/kit";
import type { Handle } from "@sveltejs/kit";

export const handle: Handle = async ({ event, resolve }) => {
  configureApiClient({
    publicApiBase: publicEnv.PUBLIC_API_BASE,
    serverFallbackBaseUrl: getBackendBaseUrl(event.platform),
  });

  const locale = getPreferredLocale(
    event.cookies.get(LOCALE_COOKIE_NAME),
    event.request.headers.get("accept-language"),
  );
  const resolveWithLocale: typeof resolve = (event, options) =>
    resolve(event, {
      ...options,
      filterSerializedResponseHeaders: (name, value) =>
        options?.filterSerializedResponseHeaders?.(name, value) ||
        name.toLowerCase() === "content-type",
      transformPageChunk: async (input) => {
        const html = (await options?.transformPageChunk?.(input)) ?? input.html;
        return html.replace("%lang%", locale);
      },
    });

  const { auth, authEnabled, settingsResponse, settingsResponseFresh } = await getAuthContext(
    event.request,
    event.platform,
  );
  event.locals.authEnabled = authEnabled;
  event.locals.backendSettings = settingsResponse;
  event.locals.backendSettingsFresh = settingsResponseFresh;
  event.locals.publicShare = null;
  event.locals.publicShareUrl = undefined;

  if (!authEnabled) {
    event.locals.session = null;
    event.locals.user = null;
    return resolveWithLocale(event);
  }

  if (!auth) {
    event.locals.session = null;
    event.locals.user = null;
    return resolveWithLocale(event);
  }

  await ensureAuthStorageReady(authEnabled, auth);

  event.locals.session =
    (await auth?.api.getSession({
      headers: event.request.headers,
    })) ?? null;
  event.locals.user = event.locals.session?.user ?? null;
  event.locals.publicShare = await resolvePublicShareAccess({
    method: event.request.method,
    platform: event.platform,
    shareId: event.cookies.get(PUBLIC_SHARE_COOKIE_NAME),
    url: event.url,
    user: event.locals.user,
  });

  if (event.locals.publicShare) {
    event.cookies.set(PUBLIC_SHARE_COOKIE_NAME, event.locals.publicShare.share.id, {
      httpOnly: true,
      maxAge: PUBLIC_SHARE_COOKIE_MAX_AGE,
      path: "/",
      sameSite: "lax",
      secure: event.url.protocol === "https:",
    });
  }

  const publicShareAllowed =
    isPublicShareRequestAllowed(event.locals.publicShare, event.url) ||
    isPublicShareApiRequestAllowed(event.locals.publicShare, event.url, event.request.method);

  if (!event.locals.session && !isPublicPath(event.url.pathname) && !publicShareAllowed) {
    if (event.url.pathname.startsWith("/api/")) {
      throw error(401, "Authentication required");
    }
    throw redirect(302, "/login");
  }

  if (event.locals.publicShare) {
    event.locals.publicShareUrl = publicShareUrlForRecord(event.locals.publicShare.share);
  }

  const { svelteKitHandler } = await import("better-auth/svelte-kit");

  return svelteKitHandler({
    auth,
    building,
    event,
    resolve: resolveWithLocale,
  });
};
