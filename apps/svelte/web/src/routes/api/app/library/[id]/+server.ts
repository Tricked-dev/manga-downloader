import { proxyApiRequest } from "$lib/server/api/backend-client";
import type { RequestHandler } from "./$types";

const path = (id: string) => `/v1/library/${encodeURIComponent(id)}`;

export const GET: RequestHandler = async (event) => proxyApiRequest(event, path(event.params.id));

export const DELETE: RequestHandler = async (event) =>
  proxyApiRequest(event, path(event.params.id));
