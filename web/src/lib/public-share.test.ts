import { describe, expect, it } from "vitest";
import { isPublicShareViewActive } from "./public-share";

describe("public share navigation", () => {
  const auth = {
    authenticated: false,
    publicAccess: { pathname: "/library/shared", routeKey: "/library/shared", shareUrl: "/library/shared?public=1" },
  };

  it("keeps the shared reader in the public view", () => {
    expect(isPublicShareViewActive(auth, new URL("https://manga.test/read/comix/chapter?libraryId=shared"))).toBe(true);
    expect(isPublicShareViewActive(auth, new URL("https://manga.test/read/comix/chapter?libraryId=other"))).toBe(false);
    expect(isPublicShareViewActive(auth, new URL("https://manga.test/read/comix/chapter"))).toBe(false);
  });

  it("does not apply public controls to an authenticated owner's reader", () => {
    expect(isPublicShareViewActive({ ...auth, authenticated: true }, new URL("https://manga.test/read/comix/chapter?libraryId=shared"))).toBe(false);
  });
});
