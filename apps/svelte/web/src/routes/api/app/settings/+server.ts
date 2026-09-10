import { createApiProxyHandler, proxyApiRequest } from "$lib/server/api/backend-client";
import { invalidateAuthSettingsCache } from "$lib/server/auth/config";
import type { RequestHandler } from "./$types";

const handleProxy = createApiProxyHandler("/v1/settings");

export const GET: RequestHandler = async (event) => {
  if (event.locals.backendSettingsFresh && event.locals.backendSettings) {
    return Response.json(event.locals.backendSettings);
  }

  return handleProxy(event);
};

export const PUT: RequestHandler = async (event) => {
  const response = await proxyApiRequest(event, "/v1/settings");

  if (response.ok) {
    invalidateAuthSettingsCache();
  }

  return response;
};
