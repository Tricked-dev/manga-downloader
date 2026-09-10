import { afterEach, describe, expect, it, vi } from "vitest";

async function importUrlModules() {
  vi.resetModules();
  vi.stubGlobal("window", undefined);

  const base = await import("./base");
  base.configureApiClient({ publicApiBase: "https://api.example.com/v1" });

  return import("./urls");
}

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

describe("imageProxyUrl", () => {
  it("requests proxied remote images as AVIF", async () => {
    const { imageProxyUrl } = await importUrlModules();

    expect(imageProxyUrl("https://cdn.example.com/cover.jpg", undefined, { format: "avif" })).toBe(
      "/api/app/media/image?url=https%3A%2F%2Fcdn.example.com%2Fcover.jpg&format=avif",
    );
  });

  it("adds AVIF format to existing media proxy URLs", async () => {
    const { imageProxyUrl } = await importUrlModules();

    expect(
      imageProxyUrl("/v1/media/image?source=test&spec=abc", undefined, { format: "avif" }),
    ).toBe("/api/app/media/image?source=test&spec=abc&format=avif");
  });

  it("keeps existing frontend media proxy URLs on the frontend proxy", async () => {
    const { imageProxyUrl } = await importUrlModules();

    expect(
      imageProxyUrl("/api/app/media/image?source=test&spec=abc", undefined, { format: "avif" }),
    ).toBe("/api/app/media/image?source=test&spec=abc&format=avif");
  });

  it("rewrites absolute backend media proxy URLs to the frontend proxy", async () => {
    const { imageProxyUrl } = await importUrlModules();

    expect(
      imageProxyUrl("https://api.example.com/v1/media/image?source=test&spec=abc", undefined, {
        format: "avif",
      }),
    ).toBe("/api/app/media/image?source=test&spec=abc&format=avif");
  });

  it("adds responsive widths to proxied image URLs", async () => {
    const { imageProxyUrl } = await importUrlModules();

    expect(
      imageProxyUrl("https://cdn.example.com/cover.jpg", undefined, {
        format: "avif",
        width: 384,
      }),
    ).toBe(
      "/api/app/media/image?url=https%3A%2F%2Fcdn.example.com%2Fcover.jpg&format=avif&width=384",
    );
  });

  it("builds a width descriptor srcset for proxied images", async () => {
    const { imageProxySrcSet } = await importUrlModules();

    expect(
      imageProxySrcSet("https://cdn.example.com/cover.jpg", undefined, [384, 192, 384], {
        format: "avif",
      }),
    ).toBe(
      "/api/app/media/image?url=https%3A%2F%2Fcdn.example.com%2Fcover.jpg&format=avif&width=192 192w, /api/app/media/image?url=https%3A%2F%2Fcdn.example.com%2Fcover.jpg&format=avif&width=384 384w",
    );
  });

  it("does not alter data URLs", async () => {
    const { imageProxyUrl } = await importUrlModules();

    expect(imageProxyUrl("data:image/png;base64,abc", undefined, { format: "avif" })).toBe(
      "data:image/png;base64,abc",
    );
  });
});
