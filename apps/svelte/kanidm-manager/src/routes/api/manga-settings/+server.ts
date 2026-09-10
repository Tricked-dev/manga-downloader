import { readMangaSettings, updateMangaSettings } from "$lib/server/manga-settings";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async ({ fetch, platform }) => {
  return Response.json(await readMangaSettings(fetch, platform));
};

export const PUT: RequestHandler = async ({ fetch, platform, request }) => {
  return updateMangaSettings(fetch, platform, await request.json());
};
