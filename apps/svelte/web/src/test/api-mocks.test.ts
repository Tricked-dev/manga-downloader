import {
  getDownloads,
  listSources,
  searchSource,
  updateSettings,
} from "@manga-server/api-client/generated";
import {
  apiMockServer,
  getGetDownloadsMockHandler,
  getListSourcesMockHandler,
  getSearchSourceMockHandler,
  getUpdateSettingsMockHandler,
} from "@manga-server/api-client/mocks";
import { describe, expect, it } from "vitest";

describe("generated API mocks", () => {
  it("lets tests provide deterministic source catalog data", async () => {
    apiMockServer.use(
      getListSourcesMockHandler({
        items: [
          {
            base_url: "https://manga.example",
            build_metadata: null,
            capabilities: ["search", "pages"],
            default_search_category: "popular",
            display_name: "Example Manga",
            enabled: true,
            homepage: "https://manga.example/home",
            name: "example",
            plugin_api_version: 1,
            plugin_version: "1.2.3",
            search_categories: ["popular", "latest"],
            source_repository: null,
            supports_search_popularity: true,
          },
        ],
      }),
    );

    const response = await listSources();

    expect(response.items).toEqual([
      expect.objectContaining({
        default_search_category: "popular",
        display_name: "Example Manga",
        enabled: true,
        name: "example",
        search_categories: ["popular", "latest"],
      }),
    ]);
  });

  it("can model an empty downloads queue", async () => {
    apiMockServer.use(getGetDownloadsMockHandler({ items: [] }));

    const response = await getDownloads();

    expect(response.items).toEqual([]);
  });

  it("captures settings update payloads and returns normalized settings", async () => {
    let requestPayload: unknown;

    apiMockServer.use(
      getUpdateSettingsMockHandler(async ({ request }) => {
        requestPayload = await request.json();

        return {
          settings: {
            auto_download_new_chapters: "true",
            library_categories: "Reading,Complete",
          },
        };
      }),
    );

    const response = await updateSettings({
      auto_download_new_chapters: "true",
      library_categories: "Reading,Complete",
    });

    expect(requestPayload).toEqual({
      auto_download_new_chapters: "true",
      library_categories: "Reading,Complete",
    });
    expect(response.settings).toEqual({
      auto_download_new_chapters: "true",
      library_categories: "Reading,Complete",
    });
  });

  it("passes source search params through the generated handler", async () => {
    let requestUrl: URL | undefined;

    apiMockServer.use(
      getSearchSourceMockHandler(({ request }) => {
        requestUrl = new URL(request.url);

        return {
          has_next_page: requestUrl.searchParams.get("page") === "1",
          mangas: [
            {
              author: "ONE",
              cover_fetch_spec: null,
              cover_proxy_url: null,
              cover_url: "https://manga.example/cover.jpg",
              description: "A deterministic mocked search result",
              genres: ["Action"],
              id: "one-punch-man",
              is_nsfw: false,
              source_base_url: "https://manga.example",
              status: "ongoing",
              title: "One Punch Man",
            },
          ],
        };
      }),
    );

    const response = await searchSource("example", {
      category: "popular",
      page: 1,
      q: "one punch",
    });

    expect(requestUrl?.pathname).toBe("/v1/sources/example/search");
    expect(requestUrl?.searchParams.get("category")).toBe("popular");
    expect(requestUrl?.searchParams.get("page")).toBe("1");
    expect(requestUrl?.searchParams.get("q")).toBe("one punch");
    expect(response).toEqual({
      has_next_page: true,
      mangas: [
        expect.objectContaining({
          id: "one-punch-man",
          title: "One Punch Man",
        }),
      ],
    });
  });
});
