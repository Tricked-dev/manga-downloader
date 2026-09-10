import { authDisabledSessionResponse } from "$lib/server/auth/session";
import { getAuthContext } from "$lib/server/auth/config";
import { json } from "@sveltejs/kit";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async ({ locals, platform, request }) => {
  const { authEnabled } = await getAuthContext(request, platform);

  if (!authEnabled) {
    return authDisabledSessionResponse();
  }

  return json({
    authEnabled: true,
    authenticated: Boolean(locals.session),
    session: locals.session,
  });
};
