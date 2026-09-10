import { afterEach, describe, expect, it, vi } from "vitest";

import { fetchClientPackage, saveBlobAsFile } from "./download-package";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("fetchClientPackage", () => {
  it("fetches the configured package href", async () => {
    const fetcher = vi.fn(async () => new Response(new Blob(["package-bytes"]), { status: 200 }));

    await fetchClientPackage(
      { href: "/v1/clients/aidoku/package", filename: "package.aix" },
      fetcher,
    );

    expect(fetcher).toHaveBeenCalledWith("/v1/clients/aidoku/package");
  });

  it("returns a blob for a successful package response", async () => {
    const blob = await fetchClientPackage(
      { href: "/v1/clients/aidoku/package", filename: "package.aix" },
      async () => new Response(new Blob(["package-bytes"]), { status: 200 }),
    );

    expect(blob.size).toBe(13);
  });

  it("throws the backend error message instead of returning an error blob", async () => {
    await expect(
      fetchClientPackage(
        { href: "/v1/clients/aidoku/package", filename: "package.aix" },
        async () =>
          Response.json(
            {
              error: {
                code: "internal_error",
                message: "internal server error",
              },
            },
            { status: 500 },
          ),
      ),
    ).rejects.toThrow("internal server error");
  });

  it("falls back to the HTTP status text when the backend does not return JSON", async () => {
    await expect(
      fetchClientPackage(
        { href: "/v1/clients/aidoku/package", filename: "package.aix" },
        async () => new Response("not found", { status: 404, statusText: "Not Found" }),
      ),
    ).rejects.toThrow("Not Found");
  });

  it("rejects empty package responses", async () => {
    await expect(
      fetchClientPackage(
        { href: "/v1/clients/aidoku/package", filename: "package.aix" },
        async () => new Response(new Blob(), { status: 200 }),
      ),
    ).rejects.toThrow("package.aix was empty");
  });
});

describe("saveBlobAsFile", () => {
  it("downloads the blob through a temporary anchor and revokes the object URL", () => {
    const anchor = {
      click: vi.fn(),
      download: "",
      href: "",
      rel: "",
      remove: vi.fn(),
      style: { display: "" },
    };
    const append = vi.fn();
    const createObjectURL = vi.fn(() => "blob:package");
    const revokeObjectURL = vi.fn();

    vi.stubGlobal("document", {
      body: { append },
      createElement: vi.fn(() => anchor),
    });
    vi.stubGlobal("URL", {
      createObjectURL,
      revokeObjectURL,
    });

    const blob = new Blob(["package-bytes"]);
    saveBlobAsFile(blob, "package.aix");

    expect(createObjectURL).toHaveBeenCalledWith(blob);
    expect(anchor.href).toBe("blob:package");
    expect(anchor.download).toBe("package.aix");
    expect(anchor.rel).toBe("noopener");
    expect(anchor.style.display).toBe("none");
    expect(append).toHaveBeenCalledWith(anchor);
    expect(anchor.click).toHaveBeenCalledOnce();
    expect(anchor.remove).toHaveBeenCalledOnce();
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:package");
  });
});
