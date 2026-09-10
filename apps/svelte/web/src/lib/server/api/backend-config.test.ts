import { env } from "$env/dynamic/private";
import { afterEach, describe, expect, it } from "vitest";

import { getBackendBaseUrl } from "./backend-config";

const originalEnv = { ...env };

function platformWithEnv(env: Record<string, string | undefined>): App.Platform {
  return { env } as unknown as App.Platform;
}

describe("backend config", () => {
  afterEach(() => {
    for (const key of Object.keys(env)) {
      delete env[key];
    }
    Object.assign(env, originalEnv);
  });

  it("prefers loopback platform BACKEND_URL over platform BACKEND_INTERNAL_URL", () => {
    expect(
      getBackendBaseUrl(
        platformWithEnv({
          BACKEND_INTERNAL_URL: "http://100.96.0.8",
          BACKEND_URL: "http://127.0.0.1:4000/",
        }),
      ),
    ).toBe("http://127.0.0.1:4000");
  });

  it("keeps dynamic BACKEND_INTERNAL_URL as the highest-priority backend", () => {
    env.BACKEND_INTERNAL_URL = "http://local-internal/";
    env.BACKEND_URL = "http://127.0.0.1:4000";

    expect(
      getBackendBaseUrl(
        platformWithEnv({
          BACKEND_INTERNAL_URL: "https://platform-internal.example.test",
        }),
      ),
    ).toBe("http://local-internal");
  });

  it("uses platform BACKEND_INTERNAL_URL ahead of non-loopback platform BACKEND_URL", () => {
    expect(
      getBackendBaseUrl(
        platformWithEnv({
          BACKEND_INTERNAL_URL: "https://platform-internal.example.test/",
          BACKEND_URL: "https://platform-public.example.test",
        }),
      ),
    ).toBe("https://platform-internal.example.test");
  });
});
