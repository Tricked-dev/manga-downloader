import { createApiProxyHandler } from "$lib/server/api/backend-client";

const handleProxy = createApiProxyHandler("/v1/settings/archive-index");

export const GET = handleProxy;
export const DELETE = handleProxy;
