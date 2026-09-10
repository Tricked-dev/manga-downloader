import { createApiProxyHandler } from "$lib/server/api/backend-client";

export const DELETE = createApiProxyHandler("/v1/downloads/failed");
