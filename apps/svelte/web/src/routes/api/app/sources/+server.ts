import { createApiProxyHandler } from "$lib/server/api/backend-client";

const handleProxy = createApiProxyHandler("/v1/sources");

export const GET = handleProxy;
