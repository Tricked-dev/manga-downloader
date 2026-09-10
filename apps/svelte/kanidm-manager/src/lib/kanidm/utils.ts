import type {
  AddNotification,
  KanidmEntry,
  KanidmProxyRequest,
  KanidmProxyResponse,
} from "./types";

export function attr(entry: KanidmEntry | undefined, key: string): string[] {
  return entry?.attrs?.[key] ?? [];
}

export function firstAttr(entry: KanidmEntry | undefined, key: string, fallback = ""): string {
  return attr(entry, key)[0] ?? fallback;
}

export function entryName(entry: KanidmEntry): string {
  return firstAttr(entry, "name");
}

export function appDisplayName(entry: KanidmEntry): string {
  return firstAttr(entry, "displayname", entryName(entry));
}

export function isPublicClient(entry: KanidmEntry | undefined): boolean {
  return attr(entry, "class").includes("oauth2_resource_server_public");
}

export function parseMultilineUrls(value: string): string[] {
  return value
    .split(/\n+/)
    .map((url) => url.trim())
    .filter((url) => /^https?:\/\//i.test(url));
}

export function parseCommaList(value: string): string[] {
  return value
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean);
}

export function readableError(value: unknown, fallback = "Request failed"): string {
  if (typeof value === "string" && value.trim()) {
    return value;
  }
  if (value && typeof value === "object") {
    const object = value as Record<string, unknown>;
    for (const key of ["invalidattribute", "message", "error"]) {
      if (typeof object[key] === "string" && object[key].trim()) {
        return object[key];
      }
    }
  }
  return fallback;
}

export async function kanidmRequest<T>(
  fetcher: typeof fetch,
  request: KanidmProxyRequest,
): Promise<KanidmProxyResponse<T>> {
  const response = await fetcher("/api/kanidm", {
    body: JSON.stringify(request),
    headers: {
      "content-type": "application/json",
    },
    method: "POST",
  });

  return response.json();
}

export async function runKanidmMutation<T>(
  fetcher: typeof fetch,
  addNotification: AddNotification,
  request: KanidmProxyRequest,
  messages: {
    error: string;
    success: string;
  },
): Promise<KanidmProxyResponse<T> | null> {
  const response = await kanidmRequest<T>(fetcher, request);
  if (response.status === 200) {
    addNotification("success", messages.success);
    return response;
  }

  addNotification("error", readableError(response.body, messages.error));
  return null;
}

export function callbackUrl(origin: string, providerId = "oidc"): string {
  return `${origin.replace(/\/+$/, "")}/api/auth/callback/${providerId || "oidc"}`;
}
