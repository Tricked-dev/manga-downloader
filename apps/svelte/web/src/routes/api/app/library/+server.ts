import { createApiProxyHandler } from "$lib/server/api/backend-client";

const handleProxy = createApiProxyHandler("/v1/library");

export const GET = handleProxy;
export const POST = handleProxy;
