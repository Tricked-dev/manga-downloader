import { createApiProxyHandler } from "$lib/server/api/backend-client";

const handleProxy = createApiProxyHandler("/v1/health");

export const GET = handleProxy;
