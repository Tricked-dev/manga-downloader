import { getAuthContext } from "$lib/server/auth/config";
import { error } from "@sveltejs/kit";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async ({ fetch, platform, request }) => {
  const { authEnabled, oidcProviderId } = await getAuthContext(request, platform);

  if (!authEnabled) {
    throw error(404, "Authentication is disabled");
  }

  const response = await fetch("/api/auth/sign-in/oauth2", {
    body: JSON.stringify({
      callbackURL: "/",
      providerId: oidcProviderId,
    }),
    headers: {
      "content-type": "application/json",
    },
    method: "POST",
  });

  const payload = (await response.json().catch(() => null)) as unknown;
  const redirectUrl =
    payload && typeof payload === "object" && typeof (payload as { url?: unknown }).url === "string"
      ? (payload as { url: string }).url
      : undefined;
  if (!response.ok || !redirectUrl) {
    throw error(response.status || 500, "Failed to start OIDC sign-in");
  }

  return new Response(null, {
    headers: {
      location: redirectUrl,
    },
    status: 302,
  });
};
