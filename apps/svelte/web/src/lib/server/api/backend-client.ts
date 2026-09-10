import { error } from "@sveltejs/kit";
import type { RequestEvent, RequestHandler } from "@sveltejs/kit";
import { getAuthContext } from "$lib/server/auth/config";
import { isPublicShareApiRequestAllowed } from "$lib/server/auth/public-share";
import { applyBackendAuthorization, getBackendBaseUrl, getBackendFetch } from "./backend-config";

interface PrivateCacheOptions {
  maxAge: number;
  staleWhileRevalidate?: number;
}

interface PublicCacheOptions {
  maxAge: number;
  edgeMaxAge?: number;
  staleWhileRevalidate?: number;
}

async function ensureApiSession(event: RequestEvent): Promise<void> {
  const authEnabled =
    event.locals.authEnabled ?? (await getAuthContext(event.request, event.platform)).authEnabled;

  if (
    authEnabled &&
    !event.locals.session &&
    !isPublicShareApiRequestAllowed(event.locals.publicShare, event.url, event.request.method)
  ) {
    throw error(401, "Authentication required");
  }
}

async function proxyToBackend(
  event: RequestEvent,
  path: string,
  init?: RequestInit,
): Promise<Response> {
  const upstreamUrl = new URL(`${getBackendBaseUrl(event.platform)}${path}`);
  upstreamUrl.search = event.url.search;
  const backendFetch = getBackendFetch(event.platform);
  const upstreamHeaders = new Headers(init?.headers ?? event.request.headers);
  upstreamHeaders.delete("authorization");
  upstreamHeaders.delete("host");
  upstreamHeaders.delete("connection");
  upstreamHeaders.delete("content-length");
  upstreamHeaders.delete("cookie");
  applyBackendAuthorization(upstreamHeaders, event.platform);

  try {
    const upstreamResponse = await backendFetch(upstreamUrl, {
      body:
        init?.body ??
        (event.request.method === "GET" || event.request.method === "HEAD"
          ? undefined
          : await event.request.arrayBuffer()),
      headers: upstreamHeaders,
      method: init?.method ?? event.request.method,
      redirect: "manual",
    });

    const responseHeaders = new Headers(upstreamResponse.headers);
    responseHeaders.delete("content-encoding");
    responseHeaders.delete("content-length");

    return new Response(upstreamResponse.body, {
      headers: responseHeaders,
      status: upstreamResponse.status,
      statusText: upstreamResponse.statusText,
    });
  } catch {
    return Response.json(
      {
        error: {
          code: "backend_unavailable",
          message: "Backend unavailable",
        },
      },
      { status: 503 },
    );
  }
}

export async function proxyApiRequest(
  event: RequestEvent,
  path: string,
  init?: RequestInit,
): Promise<Response> {
  await ensureApiSession(event);
  return proxyToBackend(event, path, init);
}

export function createApiProxyHandler(path: string, init?: RequestInit): RequestHandler {
  return async (event) => proxyApiRequest(event, path, init);
}

export function applyPrivateCacheHeaders(
  response: Response,
  options: PrivateCacheOptions,
): Response {
  if (!response.ok) {
    return response;
  }

  const directives = [`private`, `max-age=${options.maxAge}`];
  if (options.staleWhileRevalidate && options.staleWhileRevalidate > 0) {
    directives.push(`stale-while-revalidate=${options.staleWhileRevalidate}`);
  }

  response.headers.set("Cache-Control", directives.join(", "));
  return response;
}

export function applyPublicCacheHeaders(response: Response, options: PublicCacheOptions): Response {
  if (!response.ok) {
    return response;
  }

  const edgeMaxAge = options.edgeMaxAge ?? options.maxAge;
  const staleWhileRevalidate = options.staleWhileRevalidate ?? options.maxAge;
  const browserDirectives = [`public`, `max-age=${options.maxAge}`, `s-maxage=${edgeMaxAge}`];
  const cdnDirectives = [`public`, `s-maxage=${edgeMaxAge}`];
  if (staleWhileRevalidate > 0) {
    browserDirectives.push(`stale-while-revalidate=${staleWhileRevalidate}`);
    cdnDirectives.push(`stale-while-revalidate=${staleWhileRevalidate}`);
  }

  response.headers.set("Cache-Control", browserDirectives.join(", "));
  response.headers.set("CDN-Cache-Control", cdnDirectives.join(", "));
  response.headers.set("Cloudflare-CDN-Cache-Control", cdnDirectives.join(", "));
  return response;
}
