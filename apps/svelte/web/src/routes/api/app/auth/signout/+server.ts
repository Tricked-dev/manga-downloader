import { getAuthContext } from "$lib/server/auth/config";
import { json } from "@sveltejs/kit";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async ({ fetch, platform, request }) => {
  const { authEnabled } = await getAuthContext(request, platform);

  if (!authEnabled) {
    return new Response(null, { status: 204 });
  }

  const response = await fetch("/api/auth/sign-out", {
    method: "POST",
  });

  if (!response.ok) {
    const message = await response.text();
    return json({ error: message || "Failed to sign out" }, { status: response.status });
  }

  return new Response(null, {
    headers: response.headers,
    status: 204,
  });
};
