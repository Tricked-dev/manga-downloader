import { LOCALE_COOKIE_NAME, getPreferredLocale } from "$lib/i18n";
import { getAuthContext } from "$lib/server/auth/config";
import type { LayoutServerLoad } from "./$types";

export const load = (async ({ cookies, locals, platform, request }) => {
  const locale = getPreferredLocale(
    cookies.get(LOCALE_COOKIE_NAME),
    request.headers.get("accept-language"),
  );
  const { authEnabled } = await getAuthContext(request, platform);

  return {
    auth: {
      authenticated: Boolean(locals.session),
      enabled: authEnabled,
      publicAccess:
        !locals.session && locals.publicShare
          ? {
              pathname: locals.publicShare.share.pathname,
              routeKey: locals.publicShare.share.routeKey,
              shareUrl: locals.publicShareUrl ?? "",
            }
          : null,
      user: locals.user
        ? {
            email: locals.user.email,
            id: locals.user.id,
            name: locals.user.name,
          }
        : null,
    },
    locale,
  };
}) satisfies LayoutServerLoad;
