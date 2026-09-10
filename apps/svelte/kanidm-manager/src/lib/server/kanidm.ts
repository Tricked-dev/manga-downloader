import { privateEnv, readJsonObject, trimTrailingSlash } from "./env";

export interface KanidmResponse<T = unknown> {
  status: number;
  body: T;
  contentType: string;
}

export interface KanidmRequest {
  method?: "GET" | "POST" | "PATCH" | "DELETE" | "PUT";
  path: string;
  body?: unknown;
}

interface CachedToken {
  expiresAt: number;
  token: string;
}

const TOKEN_CACHE_DURATION_MS = 5 * 60 * 1000;
const JSON_HEADERS = { "content-type": "application/json" };

let tokenCache: CachedToken | null = null;

export function getKanidmBaseUrl(platform?: App.Platform): string {
  return trimTrailingSlash(privateEnv(platform, "KANIDM_BASE_URL"));
}

export function getKanidmConfigState(platform?: App.Platform) {
  return {
    baseUrl: getKanidmBaseUrl(platform),
    configured: Boolean(
      getKanidmBaseUrl(platform) &&
      privateEnv(platform, "KANIDM_USERNAME") &&
      privateEnv(platform, "KANIDM_PASSWORD"),
    ),
  };
}

export async function kanidmRequest<T = unknown>(
  fetcher: typeof fetch,
  platform: App.Platform | undefined,
  request: KanidmRequest,
): Promise<KanidmResponse<T>> {
  const baseUrl = getKanidmBaseUrl(platform);
  if (!baseUrl) {
    throw new Error("KANIDM_BASE_URL is not configured");
  }

  const token = await getCachedToken(fetcher, platform);
  const headers: Record<string, string> = {
    Authorization: `Bearer ${token}`,
  };

  let body: BodyInit | undefined;
  if (request.body !== undefined) {
    headers["content-type"] = "application/json";
    body = JSON.stringify(request.body);
  }

  const response = await fetcher(`${baseUrl}/${normalizePath(request.path)}`, {
    body,
    headers,
    method: request.method ?? "GET",
  });
  const contentType = response.headers.get("content-type") ?? "";

  return {
    body: (await readResponseBody(response, contentType)) as T,
    contentType,
    status: response.status,
  };
}

export async function kanidmFormRequest<T = unknown>(
  fetcher: typeof fetch,
  platform: App.Platform | undefined,
  path: string,
  formData: FormData,
): Promise<KanidmResponse<T>> {
  const baseUrl = getKanidmBaseUrl(platform);
  if (!baseUrl) {
    throw new Error("KANIDM_BASE_URL is not configured");
  }

  const response = await fetcher(`${baseUrl}/${normalizePath(path)}`, {
    body: formData,
    headers: {
      Authorization: `Bearer ${await getCachedToken(fetcher, platform)}`,
    },
    method: "POST",
  });
  const contentType = response.headers.get("content-type") ?? "";

  return {
    body: (await readResponseBody(response, contentType)) as T,
    contentType,
    status: response.status,
  };
}

export async function kanidmImage(
  fetcher: typeof fetch,
  platform: App.Platform | undefined,
  appName: string,
): Promise<Response> {
  const baseUrl = getKanidmBaseUrl(platform);
  if (!baseUrl) {
    return Response.json({ error: "KANIDM_BASE_URL is not configured" }, { status: 500 });
  }

  const response = await fetcher(`${baseUrl}/v1/oauth2/${encodeURIComponent(appName)}/_image`, {
    headers: {
      Authorization: `Bearer ${await getCachedToken(fetcher, platform)}`,
    },
  });

  if (!response.ok) {
    return Response.json({ error: await response.text() }, { status: response.status });
  }

  return new Response(await response.arrayBuffer(), {
    headers: {
      "cache-control": "private, max-age=300",
      "content-type": response.headers.get("content-type") ?? "application/octet-stream",
    },
  });
}

async function getCachedToken(fetcher: typeof fetch, platform?: App.Platform): Promise<string> {
  const now = Date.now();
  if (tokenCache && tokenCache.expiresAt > now) {
    return tokenCache.token;
  }

  const username = privateEnv(platform, "KANIDM_USERNAME");
  const password = privateEnv(platform, "KANIDM_PASSWORD");
  const baseUrl = getKanidmBaseUrl(platform);
  if (!username || !password) {
    throw new Error("KANIDM_USERNAME and KANIDM_PASSWORD are required");
  }

  const init = await postAuthStep(fetcher, baseUrl, {
    init2: {
      issue: "token",
      privileged: true,
      username,
    },
  });
  const sessionId = init.headers.get("x-kanidm-auth-session-id");
  if (!sessionId) {
    throw new Error("Kanidm did not return an auth session id");
  }

  await postAuthStep(fetcher, baseUrl, { begin: "password" }, sessionId);
  const auth = await postAuthStep(fetcher, baseUrl, { cred: { password } }, sessionId);
  const body = readJsonObject(await auth.json().catch(() => null));
  const state = readJsonObject(body.state);
  const token = typeof state.success === "string" ? state.success : "";
  if (!token) {
    throw new Error("Kanidm login did not return a bearer token");
  }

  tokenCache = {
    expiresAt: now + TOKEN_CACHE_DURATION_MS,
    token,
  };
  return token;
}

function postAuthStep(
  fetcher: typeof fetch,
  baseUrl: string,
  step: Record<string, unknown>,
  sessionId?: string,
) {
  return fetcher(`${baseUrl}/v1/auth`, {
    body: JSON.stringify({ step }),
    headers: sessionId ? { ...JSON_HEADERS, "x-kanidm-auth-session-id": sessionId } : JSON_HEADERS,
    method: "POST",
  });
}

async function readResponseBody(response: Response, contentType: string): Promise<unknown> {
  if (response.status === 204) {
    return null;
  }
  if (contentType.includes("json")) {
    return response.json().catch(() => null);
  }
  return response.text();
}

function normalizePath(path: string): string {
  return path.replace(/^\/+/, "");
}
