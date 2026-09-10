import { applyPublicCacheHeaders, proxyApiRequest } from "$lib/server/api/backend-client";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = (event) =>
  proxyApiRequest(event, "/v1/media/image").then((response) =>
    applyPublicCacheHeaders(response, {
      maxAge: 24 * 60 * 60,
    }),
  );
