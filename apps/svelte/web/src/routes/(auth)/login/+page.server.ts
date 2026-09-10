import { redirect } from "@sveltejs/kit";
import { getAuthContext } from "$lib/server/auth/config";
import type { PageServerLoad } from "./$types";

export const load: PageServerLoad = async ({ locals, platform, request }) => {
  const { authEnabled } = await getAuthContext(request, platform);

  if (!authEnabled || locals.session) {
    throw redirect(302, "/");
  }
};
