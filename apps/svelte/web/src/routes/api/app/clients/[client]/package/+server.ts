import { applyPublicCacheHeaders, proxyApiRequest } from "$lib/server/api/backend-client";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = (event) => {
  const client = encodeURIComponent(event.params.client);
  return proxyApiRequest(event, `/v1/clients/${client}/package`).then((response) =>
    applyPublicCacheHeaders(response, {
      maxAge: 60 * 60,
    }),
  );
};
