import { proxyApiRequest } from "$lib/server/api/backend-client";
import type { RequestHandler } from "./$types";

export const POST: RequestHandler = async (event) =>
  proxyApiRequest(event, `/v1/library/${encodeURIComponent(event.params.id)}/chapters/refresh`);
