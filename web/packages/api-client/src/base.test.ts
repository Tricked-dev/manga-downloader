import { afterEach, describe, expect, it, vi } from "vitest";

async function importBaseModule(publicApiBase?: string) {
  vi.resetModules();
  vi.stubEnv("PUBLIC_API_BASE", publicApiBase);

  return import("./base");
}

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

describe("getApiBase", () => {
  it("uses the app-relative API path in the browser", async () => {
    vi.stubGlobal("window", {});

    const { API_BASE, getApiBase } = await importBaseModule("https://api.example.com/");

    expect(getApiBase()).toBe("/v1");
    expect(API_BASE).toBe("/v1");
  });

  it("uses a trimmed absolute PUBLIC_API_BASE on the server", async () => {
    vi.stubGlobal("window", undefined);

    const { API_BASE, getApiBase } = await importBaseModule("https://api.example.com/");

    expect(getApiBase()).toBe("https://api.example.com");
    expect(API_BASE).toBe("https://api.example.com");
  });

  it("accepts an absolute http PUBLIC_API_BASE on the server", async () => {
    vi.stubGlobal("window", undefined);

    const { getApiBase } = await importBaseModule("http://localhost:9000/");

    expect(getApiBase()).toBe("http://localhost:9000");
  });

  it.each([["api.example.com"], [""], [undefined]])(
    "falls back to localhost when PUBLIC_API_BASE is %s",
    async (publicApiBase) => {
      vi.stubGlobal("window", undefined);

      const { getApiBase } = await importBaseModule(publicApiBase);

      expect(getApiBase()).toBe("http://localhost:4000");
    },
  );

  it("uses explicit runtime configuration before process env", async () => {
    vi.stubGlobal("window", undefined);

    const { configureApiClient, getApiBase } = await importBaseModule("https://env.example.com");
    configureApiClient({ publicApiBase: "https://configured.example.com/" });

    expect(getApiBase()).toBe("https://configured.example.com");
  });

  it("uses the configured server fallback when no public API base is set", async () => {
    vi.stubGlobal("window", undefined);

    const { configureApiClient, getApiBase } = await importBaseModule();
    configureApiClient({ serverFallbackBaseUrl: "http://192.168.1.20:4000/" });

    expect(getApiBase()).toBe("http://192.168.1.20:4000");
  });
});
