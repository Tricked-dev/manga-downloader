import { afterEach, describe, expect, it, vi } from "vitest";
import { dehydrate } from "@tanstack/query-core";

import {
  QUERY_CACHE_TIMES,
  createAppQueryClient,
  isFullDownloadsQueryKey,
  shouldRetryAppQuery,
  visibleRefetchInterval,
} from "./query-client";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("createAppQueryClient", () => {
  it("uses conservative frontend query defaults", () => {
    const queryClient = createAppQueryClient();

    expect(queryClient.getDefaultOptions().queries).toEqual(
      expect.objectContaining({
        enabled: true,
        gcTime: QUERY_CACHE_TIMES.inactive,
        refetchOnWindowFocus: false,
        retry: shouldRetryAppQuery,
        staleTime: QUERY_CACHE_TIMES.live,
      }),
    );
  });

  it("can disable observer queries for non-browser hydration setup", () => {
    const queryClient = createAppQueryClient({ observerQueriesEnabled: false });

    expect(queryClient.getDefaultOptions().queries?.enabled).toBe(false);
  });

  it("does not dehydrate full download-list queries", () => {
    const queryClient = createAppQueryClient();
    queryClient.setQueryData(["v1", "downloads"], { items: [{ id: "download-1" }] });
    queryClient.setQueryData(["v1", "library"], { items: [{ id: "manga-1" }] });

    const queryKeys = dehydrate(queryClient).queries.map((query) => query.queryKey);

    expect(queryKeys).toContainEqual(["v1", "library"]);
    expect(queryKeys).not.toContainEqual(["v1", "downloads"]);
  });

  it("does not retry client-side API errors", () => {
    expect(shouldRetryAppQuery(0, { status: 404 })).toBe(false);
    expect(shouldRetryAppQuery(0, { status: 422 })).toBe(false);
  });

  it("retries one transient failure", () => {
    expect(shouldRetryAppQuery(0, { status: 503 })).toBe(true);
    expect(shouldRetryAppQuery(1, { status: 503 })).toBe(false);
    expect(shouldRetryAppQuery(0, new TypeError("network failed"))).toBe(true);
  });
});

describe("isFullDownloadsQueryKey", () => {
  it.each([
    [["v1", "downloads"], true],
    [["infinite", "v1", "downloads"], true],
    [["v1", "downloads", { status: "active" }], false],
    [["v1", "library"], false],
  ] as const)("returns %s for %j", (queryKey, expected) => {
    expect(isFullDownloadsQueryKey(queryKey)).toBe(expected);
  });
});

describe("visibleRefetchInterval", () => {
  it("does not poll when document is unavailable", () => {
    vi.stubGlobal("document", undefined);

    expect(visibleRefetchInterval(3000)()).toBe(false);
  });

  it.each([
    ["visible", 3000],
    ["hidden", false],
    ["prerender", false],
  ])("returns %s-aware interval behavior", (visibilityState, expected) => {
    vi.stubGlobal("document", { visibilityState });

    expect(visibleRefetchInterval(3000)()).toBe(expected);
  });

  it("does not poll when the caller disables the interval", () => {
    vi.stubGlobal("document", { visibilityState: "visible" });

    expect(visibleRefetchInterval(3000, () => false)()).toBe(false);
  });
});
