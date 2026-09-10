import { createApiProxyHandler } from "$lib/server/api/backend-client";

const handleProxy = createApiProxyHandler("/v1/downloads/bulk");

export const POST = handleProxy;
