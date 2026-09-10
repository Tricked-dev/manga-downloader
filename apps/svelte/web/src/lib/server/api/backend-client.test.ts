import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RequestEvent } from "@sveltejs/kit";

import {
  applyPrivateCacheHeaders,
  applyPublicCacheHeaders,
  proxyApiRequest,
} from "./backend-client";
import { invalidateAuthSettingsCache } from "$lib/server/auth/config";

function createEvent({
  body,
  headers,
  method = "GET",
  platform,
  session = null,
  url = "http://frontend.test/api/app/downloads?status=queued",
}: {
  body?: BodyInit;
  headers?: HeadersInit;
  method?: string;
  platform?: App.Platform;
  session?: unknown;
  url?: string;
} = {}): RequestEvent {
  return {
    locals: { session },
    platform,
    request: new Request(url, { body, headers, method }),
    url: new URL(url),
  } as RequestEvent;
}

function settingsResponse(settings: Record<string, string> = {}) {
  return Response.json({ settings });
}

beforeEach(() => {
  invalidateAuthSettingsCache();
});

afterEach(() => {
  invalidateAuthSettingsCache();
  vi.restoreAllMocks();
});

describe("proxyApiRequest", () => {
  it("proxies method, query string, body, and sanitized headers to the backend", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(settingsResponse())
      .mockResolvedValueOnce(Response.json({ ok: true }, { status: 202 }));

    const response = await proxyApiRequest(
      createEvent({
        body: JSON.stringify({ manga_id: "series-1" }),
        headers: {
          authorization: "Bearer browser-token",
          "content-type": "application/json",
          cookie: "sid=browser",
          "x-request-id": "req-1",
        },
        method: "POST",
        url: "http://frontend.test/api/app/downloads?status=queued",
      }),
      "/v1/downloads",
    );

    expect(response.status).toBe(202);
    expect(await response.json()).toEqual({ ok: true });
    expect(fetchMock).toHaveBeenCalledTimes(2);

    const [upstreamUrl, upstreamInit] = fetchMock.mock.calls[1] as [URL, RequestInit];
    const upstreamHeaders = upstreamInit.headers as Headers;

    expect(upstreamUrl.toString()).toBe("http://localhost:4000/v1/downloads?status=queued");
    expect(upstreamInit.method).toBe("POST");
    expect(await new Response(upstreamInit.body).json()).toEqual({ manga_id: "series-1" });
    expect(upstreamHeaders.get("content-type")).toBe("application/json");
    expect(upstreamHeaders.get("x-request-id")).toBe("req-1");
    expect(upstreamHeaders.has("authorization")).toBe(false);
    expect(upstreamHeaders.has("cookie")).toBe(false);
  });

  it("uses platform backend config and backend API key when available", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(settingsResponse())
      .mockResolvedValueOnce(Response.json({ items: [] }));

    await proxyApiRequest(
      createEvent({
        platform: {
          env: {
            BACKEND_API_KEY: "server-secret",
            BACKEND_URL: "https://backend.internal/",
          },
        } as unknown as App.Platform,
        url: "http://frontend.test/api/app/sources?enabled=true",
      }),
      "/v1/sources",
    );

    const [settingsUrl, settingsInit] = fetchMock.mock.calls[0] as [string, RequestInit];
    const [upstreamUrl, upstreamInit] = fetchMock.mock.calls[1] as [URL, RequestInit];

    expect(settingsUrl).toBe("https://backend.internal/v1/settings");
    expect((settingsInit.headers as Headers).get("authorization")).toBe("Bearer server-secret");
    expect(upstreamUrl.toString()).toBe("https://backend.internal/v1/sources?enabled=true");
    expect((upstreamInit.headers as Headers).get("authorization")).toBe("Bearer server-secret");
  });

  it("keeps BACKEND_INTERNAL_URL as a platform fallback", async () => {
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(settingsResponse())
      .mockResolvedValueOnce(Response.json({ items: [] }));

    await proxyApiRequest(
      createEvent({
        platform: {
          env: {
            BACKEND_INTERNAL_URL: "https://backend.internal/",
          },
        } as unknown as App.Platform,
        url: "http://frontend.test/api/app/sources",
      }),
      "/v1/sources",
    );

    const [upstreamUrl] = fetchMock.mock.calls[1] as [URL, RequestInit];

    expect(upstreamUrl.toString()).toBe("https://backend.internal/v1/sources");
  });

  it("returns a stable backend-unavailable envelope when upstream fetch fails", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("connect ECONNREFUSED"));

    const response = await proxyApiRequest(createEvent(), "/v1/downloads");

    await expect(response.json()).resolves.toEqual({
      error: {
        code: "backend_unavailable",
        message: "Backend unavailable",
      },
    });
    expect(response.status).toBe(503);
  });
});

describe("applyPrivateCacheHeaders", () => {
  it("sets private cache directives on successful responses", () => {
    const response = applyPrivateCacheHeaders(Response.json({ ok: true }), {
      maxAge: 3600,
      staleWhileRevalidate: 86_400,
    });

    expect(response.headers.get("cache-control")).toBe(
      "private, max-age=3600, stale-while-revalidate=86400",
    );
  });

  it("leaves error responses uncached", () => {
    const response = applyPrivateCacheHeaders(Response.json({ error: "nope" }, { status: 500 }), {
      maxAge: 3600,
    });

    expect(response.headers.has("cache-control")).toBe(false);
  });
});

describe("applyPublicCacheHeaders", () => {
  it("sets shared cache directives for browser, CDN, and Cloudflare caches", () => {
    const response = applyPublicCacheHeaders(Response.json({ ok: true }), {
      maxAge: 3600,
    });

    expect(response.headers.get("cache-control")).toBe(
      "public, max-age=3600, s-maxage=3600, stale-while-revalidate=3600",
    );
    expect(response.headers.get("cdn-cache-control")).toBe(
      "public, s-maxage=3600, stale-while-revalidate=3600",
    );
    expect(response.headers.get("cloudflare-cdn-cache-control")).toBe(
      "public, s-maxage=3600, stale-while-revalidate=3600",
    );
  });

  it("allows a separate Cloudflare edge TTL from the browser TTL", () => {
    const response = applyPublicCacheHeaders(Response.json({ ok: true }), {
      maxAge: 300,
      edgeMaxAge: 3600,
    });

    expect(response.headers.get("cache-control")).toBe(
      "public, max-age=300, s-maxage=3600, stale-while-revalidate=300",
    );
    expect(response.headers.get("cloudflare-cdn-cache-control")).toBe(
      "public, s-maxage=3600, stale-while-revalidate=300",
    );
  });

  it("allows an explicit revalidation window when a route needs it", () => {
    const response = applyPublicCacheHeaders(Response.json({ ok: true }), {
      maxAge: 300,
      staleWhileRevalidate: 60,
    });

    expect(response.headers.get("cache-control")).toBe(
      "public, max-age=300, s-maxage=300, stale-while-revalidate=60",
    );
  });

  it("leaves error responses uncached", () => {
    const response = applyPublicCacheHeaders(Response.json({ error: "nope" }, { status: 500 }), {
      maxAge: 3600,
    });

    expect(response.headers.has("cache-control")).toBe(false);
    expect(response.headers.has("cdn-cache-control")).toBe(false);
    expect(response.headers.has("cloudflare-cdn-cache-control")).toBe(false);
  });
});
