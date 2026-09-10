import { privateEnv, trimTrailingSlash } from "./env";
import type { BetterAuthSettings } from "$lib/kanidm/types";

export function getMangaServerBaseUrl(platform?: App.Platform): string {
  return trimTrailingSlash(
    privateEnv(platform, "MANGA_SERVER_BASE_URL") ||
      privateEnv(platform, "BACKEND_INTERNAL_URL") ||
      privateEnv(platform, "PUBLIC_API_BASE"),
  );
}

export function getMangaSettingsConfigState(platform?: App.Platform) {
  return {
    baseUrl: getMangaServerBaseUrl(platform),
    configured: Boolean(getMangaServerBaseUrl(platform)),
  };
}

export async function readMangaSettings(fetcher: typeof fetch, platform?: App.Platform) {
  const baseUrl = getMangaServerBaseUrl(platform);
  if (!baseUrl) {
    return { configured: false, settings: {} as BetterAuthSettings };
  }

  let response: Response;
  try {
    response = await fetcher(`${baseUrl}/v1/settings`, {
      headers: authHeaders(platform),
    });
  } catch (error) {
    return {
      configured: true,
      error: error instanceof Error ? error.message : "Manga Server settings request failed",
      settings: {} as BetterAuthSettings,
    };
  }
  if (!response.ok) {
    return { configured: true, error: await response.text(), settings: {} as BetterAuthSettings };
  }

  const body = (await response.json().catch(() => null)) as {
    settings?: BetterAuthSettings;
  } | null;
  return { configured: true, settings: body?.settings ?? {} };
}

export async function updateMangaSettings(
  fetcher: typeof fetch,
  platform: App.Platform | undefined,
  settings: BetterAuthSettings,
) {
  const baseUrl = getMangaServerBaseUrl(platform);
  if (!baseUrl) {
    return Response.json({ error: "MANGA_SERVER_BASE_URL is not configured" }, { status: 500 });
  }

  let response: Response;
  try {
    response = await fetcher(`${baseUrl}/v1/settings`, {
      body: JSON.stringify(settings),
      headers: {
        "content-type": "application/json",
        ...authHeaders(platform),
      },
      method: "PUT",
    });
  } catch (error) {
    return Response.json(
      { error: error instanceof Error ? error.message : "Manga Server settings request failed" },
      { status: 502 },
    );
  }

  return Response.json(await response.json().catch(() => ({ error: response.statusText })), {
    status: response.status,
  });
}

function authHeaders(platform?: App.Platform): HeadersInit {
  const apiKey =
    privateEnv(platform, "MANGA_SERVER_API_KEY") || privateEnv(platform, "BACKEND_API_KEY");
  return apiKey ? { Authorization: `Bearer ${apiKey}` } : {};
}
