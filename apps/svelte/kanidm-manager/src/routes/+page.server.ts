import { getKanidmConfigState, kanidmRequest } from "$lib/server/kanidm";
import { getMangaSettingsConfigState, readMangaSettings } from "$lib/server/manga-settings";
import type { KanidmCollection } from "$lib/kanidm/types";
import type { PageServerLoad } from "./$types";

const EMPTY_COLLECTION: KanidmCollection = { body: [] };

export const load: PageServerLoad = async ({ fetch, platform, url }) => {
  const kanidm = getKanidmConfigState(platform);
  const mangaSettings = getMangaSettingsConfigState(platform);

  if (!kanidm.configured) {
    return {
      apps: EMPTY_COLLECTION,
      groups: EMPTY_COLLECTION,
      kanidm,
      mangaSettings,
      mangaSettingsState: await readMangaSettings(fetch, platform),
      origin: url.origin,
      users: EMPTY_COLLECTION,
    };
  }

  const [apps, groups, users, mangaSettingsState] = await Promise.all([
    loadCollection(fetch, platform, "v1/oauth2"),
    loadCollection(fetch, platform, "v1/group"),
    loadCollection(fetch, platform, "v1/person"),
    readMangaSettings(fetch, platform),
  ]);

  return {
    apps,
    groups,
    kanidm,
    mangaSettings,
    mangaSettingsState,
    origin: url.origin,
    users,
  };
};

async function loadCollection(
  fetcher: typeof fetch,
  platform: App.Platform | undefined,
  path: string,
): Promise<KanidmCollection> {
  try {
    const response = await kanidmRequest<KanidmCollection["body"]>(fetcher, platform, { path });
    return {
      body: Array.isArray(response.body) ? response.body : [],
      status: response.status,
    };
  } catch {
    return EMPTY_COLLECTION;
  }
}
