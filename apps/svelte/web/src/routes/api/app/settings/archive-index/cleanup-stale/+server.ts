import { createApiProxyHandler } from "$lib/server/api/backend-client";

export const POST = createApiProxyHandler("/v1/settings/archive-index/cleanup-stale");
