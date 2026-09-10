import { proxyApiRequest } from "$lib/server/api/backend-client";
import type { RequestHandler } from "./$types";

const path = (name: string) => `/v1/sources/${encodeURIComponent(name)}/settings`;

export const GET: RequestHandler = async (event) => proxyApiRequest(event, path(event.params.name));

export const PUT: RequestHandler = async (event) => proxyApiRequest(event, path(event.params.name));
