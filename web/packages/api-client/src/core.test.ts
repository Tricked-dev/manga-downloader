import { describe, expect, it, vi } from "vitest";

import { ApiError, isApiErrorCode, request } from "./core";

describe("request errors", () => {
  it("classifies backend error codes", async () => {
    const fetchImpl = async () =>
      Response.json(
        {
          error: {
            code: "downloaded_page_conflict",
            message: "downloaded archive changed while reading page",
          },
        },
        { status: 409 },
      );

    const error = await request("/library/chapters/1/pages/0", undefined, {
      baseUrl: "http://backend.test",
      fetch: fetchImpl,
    }).catch((caught: unknown) => caught);

    expect(isApiErrorCode(error, "downloaded_page_conflict")).toBe(true);
    expect(isApiErrorCode(error, "not_found")).toBe(false);
  });

  it("uses nested backend error envelopes", async () => {
    const fetchImpl = async () =>
      Response.json(
        {
          error: {
            code: "not_found",
            message: "download archive not found",
          },
        },
        { status: 404 },
      );

    await expect(
      request("/downloads/1/archive", undefined, {
        baseUrl: "http://backend.test",
        fetch: fetchImpl,
      }),
    ).rejects.toMatchObject({
      _tag: "ApiError",
      code: "not_found",
      reason: "download archive not found",
      status: 404,
    } satisfies Partial<ApiError>);
  });

  it("adds validation violations to the error reason", async () => {
    const fetchImpl = async () =>
      Response.json(
        {
          error: {
            code: "validation_error",
            details: {
              violations: [
                {
                  message: "must not be blank",
                  path: "manga_id",
                },
              ],
            },
            message: "Request validation failed",
          },
        },
        { status: 400 },
      );

    await expect(
      request("/downloads", undefined, {
        baseUrl: "http://backend.test",
        fetch: fetchImpl,
      }),
    ).rejects.toMatchObject({
      _tag: "ApiError",
      reason: "Request validation failed: manga_id: must not be blank",
      status: 400,
    } satisfies Partial<ApiError>);
  });

  it("falls back to the HTTP status text for malformed JSON error payloads", async () => {
    const fetchImpl = async () =>
      Response.json(
        { message: "chapter not found" },
        {
          status: 404,
          statusText: "Not Found",
        },
      );

    await expect(
      request("/chapter", undefined, {
        baseUrl: "http://backend.test",
        fetch: fetchImpl,
      }),
    ).rejects.toMatchObject({
      reason: "Not Found",
      status: 404,
    } satisfies Partial<ApiError>);
  });
});

describe("frontend request resilience", () => {
  it("does not inspect the body stream before reading JSON responses", async () => {
    const fetchImpl = async () => {
      const response = Response.json({ items: [] });
      let teedBody: ReadableStream<Uint8Array> | undefined;

      return new Proxy(response, {
        get(target, key, receiver) {
          if (key === "body") {
            if (target.body === null) {
              return null;
            }

            if (teedBody) {
              return teedBody;
            }

            const [, body] = target.body.tee();
            teedBody = body;
            return teedBody;
          }

          if (key === "json") {
            return async () => {
              const body = await target.text();
              return body ? JSON.parse(body) : undefined;
            };
          }

          const value = Reflect.get(target, key, target);
          return value instanceof Function ? value.bind(target) : value;
        },
      });
    };

    await expect(
      request("/library", undefined, {
        baseUrl: "http://backend.test",
        fetch: fetchImpl,
      }),
    ).resolves.toEqual({ items: [] });
  });

  it("coalesces matching concurrent GET requests", async () => {
    let resolveFetch: ((response: Response) => void) | undefined;
    const fetchImpl = vi.fn(
      () =>
        new Promise<Response>((resolve) => {
          resolveFetch = resolve;
        }),
    );

    const first = request("/info", undefined, {
      baseUrl: "http://backend.test",
      fetch: fetchImpl,
    });
    const second = request("/info", undefined, {
      baseUrl: "http://backend.test",
      fetch: fetchImpl,
    });

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    resolveFetch?.(Response.json({ version: "1" }));

    await expect(Promise.all([first, second])).resolves.toEqual([
      { version: "1" },
      { version: "1" },
    ]);
  });

  it("retries transient GET failures", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValueOnce(Response.json({ error: { message: "busy" } }, { status: 503 }))
      .mockResolvedValueOnce(Response.json({ ok: true }));

    await expect(
      request("/health", undefined, {
        baseUrl: "http://backend.test",
        fetch: fetchImpl,
      }),
    ).resolves.toEqual({ ok: true });

    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });

  it("does not retry upstream blocked failures", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      Response.json(
        {
          error: {
            code: "upstream_blocked",
            message: "upstream source blocked this server's request",
          },
        },
        { status: 502 },
      ),
    );

    await expect(
      request("/sources/comix/search", undefined, {
        baseUrl: "http://backend.test",
        fetch: fetchImpl,
      }),
    ).rejects.toMatchObject({
      code: "upstream_blocked",
      reason: "upstream source blocked this server's request",
      status: 502,
    } satisfies Partial<ApiError>);

    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });

  it("does not retry mutating requests", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      Response.json(
        { error: { message: "busy" } },
        {
          status: 503,
          statusText: "Service Unavailable",
        },
      ),
    );

    await expect(
      request(
        "/downloads",
        {
          body: JSON.stringify({ manga_id: "1" }),
          method: "POST",
        },
        {
          baseUrl: "http://backend.test",
          fetch: fetchImpl,
        },
      ),
    ).rejects.toMatchObject({ status: 503 });

    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });
});
