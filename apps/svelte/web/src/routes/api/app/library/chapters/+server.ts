import { createApiProxyHandler } from "$lib/server/api/backend-client";

export const GET = createApiProxyHandler("/v1/library/chapters");
