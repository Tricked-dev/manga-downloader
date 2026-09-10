import { env } from "$env/dynamic/private";
import { absoluteUrlOrNull, nonEmptyStringOrNull } from "$lib/server/config-values";

export type ServerFetch = (
  input: RequestInfo | URL | string,
  init?: RequestInit,
) => Promise<Response>;

export function getBackendBaseUrl(platform?: App.Platform): string {
  const platformBackendUrl = absoluteUrlOrNull(platform?.env.BACKEND_URL);
  const platformLocalBackendUrl =
    platformBackendUrl && isLoopbackBackendUrl(platformBackendUrl) ? platformBackendUrl : null;

  return (
    absoluteUrlOrNull(env.BACKEND_INTERNAL_URL) ??
    absoluteUrlOrNull(env.BACKEND_URL) ??
    absoluteUrlOrNull(env.PUBLIC_API_BASE) ??
    platformLocalBackendUrl ??
    absoluteUrlOrNull(platform?.env.BACKEND_INTERNAL_URL) ??
    platformBackendUrl ??
    absoluteUrlOrNull(platform?.env.PUBLIC_API_BASE) ??
    "http://localhost:4000"
  ).replace(/\/$/, "");
}

export function getPrivateNetworkFetch(platform?: App.Platform): ServerFetch | undefined {
  const mesh = platform?.env.TRASHCAN_MESH;
  if (!mesh) {
    return undefined;
  }

  if (isLoopbackBackendUrl(getBackendBaseUrl(platform))) {
    return undefined;
  }

  return (input, init) => mesh.fetch(input as never, init as never) as unknown as Promise<Response>;
}

export function getBackendFetch(platform?: App.Platform): ServerFetch {
  return getPrivateNetworkFetch(platform) ?? fetch;
}

export function applyBackendAuthorization(headers: Headers, platform?: App.Platform): Headers {
  const apiKey =
    nonEmptyStringOrNull(env.BACKEND_API_KEY) ??
    nonEmptyStringOrNull(platform?.env.BACKEND_API_KEY);
  if (apiKey) {
    headers.set("Authorization", `Bearer ${apiKey}`);
  }

  return headers;
}

function isLoopbackBackendUrl(value: string): boolean {
  try {
    const hostname = new URL(value).hostname;
    return hostname === "localhost" || hostname === "127.0.0.1" || hostname === "::1";
  } catch {
    return false;
  }
}
