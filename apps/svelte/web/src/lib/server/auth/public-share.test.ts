import { describe, expect, it } from "vitest";
import { isPublicShareViewActive, isShareablePublicPath } from "$lib/public-share";
import {
  isPublicShareApiRequestAllowed,
  isPublicShareRequestAllowed,
  publicShareRequestHeaders,
  publicShareUrlForRecord,
  type PublicShareAccess,
  type PublicShareRecord,
} from "./public-share";

function share(routeKey: string): PublicShareAccess {
  const url = new URL(`http://frontend.test${routeKey}`);
  const record: PublicShareRecord = {
    createdAt: "2026-05-31T00:00:00.000Z",
    createdByEmail: "owner@example.com",
    createdByUserId: "user-1",
    id: "share-1",
    lastAccessedAt: null,
    pathname: url.pathname,
    routeKey,
    search: url.search,
    updatedAt: "2026-05-31T00:00:00.000Z",
  };

  return { share: record };
}

describe("public share policy", () => {
  it("only lets library detail pages be published", () => {
    expect(isShareablePublicPath("/library/series-1")).toBe(true);
    expect(isShareablePublicPath("/library/series-1/")).toBe(true);
    expect(isShareablePublicPath("/")).toBe(false);
    expect(isShareablePublicPath("/updates")).toBe(false);
    expect(isShareablePublicPath("/sources/source-a/search")).toBe(false);
    expect(isShareablePublicPath("/read/source-a/chapter-1")).toBe(false);
  });

  it("builds share URLs with the public flag", () => {
    expect(publicShareUrlForRecord(share("/library/series-1?chapterId=chapter-1").share)).toBe(
      "/library/series-1?chapterId=chapter-1&public=true",
    );
  });

  it("only activates the public view for anonymous shared-page visits", () => {
    const publicAccess = {
      pathname: "/library/series-1",
      routeKey: "/library/series-1",
      shareUrl: "/library/series-1?public=true",
    };

    expect(
      isPublicShareViewActive(
        { authenticated: false, publicAccess },
        new URL("http://frontend.test/library/series-1?public=true"),
      ),
    ).toBe(true);
    expect(
      isPublicShareViewActive(
        { authenticated: true, publicAccess },
        new URL("http://frontend.test/library/series-1?public=true"),
      ),
    ).toBe(false);
  });

  it("carries the public share cookie into SSR API fetches", () => {
    const headers = new Headers(
      publicShareRequestHeaders(
        share("/library/series-1"),
        "session=private; manga_public_share=old-share",
      ),
    );

    expect(headers.get("cookie")).toBe("session=private; manga_public_share=share-1");
    expect(publicShareRequestHeaders(null, "session=private")).toBeUndefined();
  });

  it("allows the exact shared page route", () => {
    const access = share("/library/series-1?tab=chapters");

    expect(
      isPublicShareRequestAllowed(
        access,
        new URL("http://frontend.test/library/series-1?tab=chapters&public=true"),
      ),
    ).toBe(true);
    expect(
      isPublicShareRequestAllowed(access, new URL("http://frontend.test/library/series-2")),
    ).toBe(false);
    expect(
      isPublicShareRequestAllowed(
        access,
        new URL("http://frontend.test/read/source-a/chapter-1?chapterId=chapter-1"),
      ),
    ).toBe(false);
    expect(
      isPublicShareRequestAllowed(
        access,
        new URL(
          "http://frontend.test/read/source-a/chapter-1?chapterId=chapter-1&libraryId=series-1",
        ),
      ),
    ).toBe(true);
  });

  it("allows read-only API endpoints needed by a shared library page", () => {
    const access = share("/library/series-1");

    expect(isPublicShareApiRequestAllowed(access, "/api/app/library/series-1", "GET")).toBe(true);
    expect(
      isPublicShareApiRequestAllowed(access, "/api/app/library/series-1/chapters", "GET"),
    ).toBe(true);
    expect(isPublicShareApiRequestAllowed(access, "/api/app/media/image", "GET")).toBe(true);
    expect(isPublicShareApiRequestAllowed(access, "/api/app/settings", "GET")).toBe(false);
    expect(isPublicShareApiRequestAllowed(access, "/api/app/session", "GET")).toBe(false);
    expect(
      isPublicShareApiRequestAllowed(
        access,
        new URL("http://frontend.test/api/app/library/chapters/chapter-1/pages?libraryId=series-1"),
        "GET",
      ),
    ).toBe(true);
    expect(
      isPublicShareApiRequestAllowed(
        access,
        new URL(
          "http://frontend.test/api/app/library/chapters/chapter-1/pages/0?libraryId=series-1",
        ),
        "GET",
      ),
    ).toBe(true);
    expect(
      isPublicShareApiRequestAllowed(
        access,
        new URL("http://frontend.test/api/app/library/chapters/chapter-1/pages?libraryId=series-2"),
        "GET",
      ),
    ).toBe(false);
    expect(
      isPublicShareApiRequestAllowed(access, "/api/app/library/chapters/chapter-1/pages", "GET"),
    ).toBe(false);
    expect(isPublicShareApiRequestAllowed(access, "/api/app/library/series-1", "POST")).toBe(false);
  });

  it("does not expose source manga API data through public shares", () => {
    const access = share("/manga/source-a/manga-1");

    expect(
      isPublicShareApiRequestAllowed(access, "/api/app/sources/source-a/manga/manga-1", "GET"),
    ).toBe(false);
    expect(
      isPublicShareApiRequestAllowed(
        access,
        "/api/app/sources/source-a/manga/manga-1/chapters",
        "GET",
      ),
    ).toBe(false);
    expect(
      isPublicShareApiRequestAllowed(access, "/api/app/sources/source-a/manga/manga-2", "GET"),
    ).toBe(false);
  });
});
