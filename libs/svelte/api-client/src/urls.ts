import { getApiBase, getBrowserApiBase } from "./base";

function resolveVersionedApiUrl(path: string): string {
  const apiBase = getApiBase();

  if (apiBase.startsWith("http://") || apiBase.startsWith("https://")) {
    if (apiBase.endsWith("/v1")) {
      return `${apiBase}${path.startsWith("/v1") ? path.slice(3) : path}`;
    }

    return `${apiBase}${path}`;
  }

  return `${apiBase}${path.replace(/^\/v1/, "")}`;
}

export const apiResourceUrl = (path: string): string => resolveVersionedApiUrl(path);

export const downloadsArchiveUrl = (id: string) =>
  resolveVersionedApiUrl(`/v1/downloads/${id}/archive`);

function resolveFrontendApiUrl(path: string): string {
  return `${getBrowserApiBase()}${path.replace(/^\/v1/, "")}`;
}

export type ImageProxyFormat = "avif";

export type ImageProxyOptions = {
  format?: ImageProxyFormat;
  width?: number;
};

export const imageProxyUrl = (
  url: string | undefined,
  fallbackBaseUrl?: string | null,
  options: ImageProxyOptions = {},
): string => {
  if (!url) {
    return "";
  }
  if (url.startsWith("blob:") || url.startsWith("data:")) {
    return url;
  }
  const existingProxyPath = imageProxyPath(url);
  if (existingProxyPath) {
    return appendImageProxyOptions(resolveFrontendApiUrl(existingProxyPath), options);
  }

  return appendImageProxyOptions(
    resolveFrontendApiUrl(
      `/v1/media/image?url=${encodeURIComponent(normalizeAbsoluteUrl(url, fallbackBaseUrl))}`,
    ),
    options,
  );
};

export const imageProxySrcSet = (
  url: string | undefined,
  fallbackBaseUrl: string | null | undefined,
  widths: readonly number[],
  options: Omit<ImageProxyOptions, "width"> = {},
): string | undefined => {
  if (!url || url.startsWith("blob:") || url.startsWith("data:")) {
    return undefined;
  }

  const candidates = normalizeImageWidths(widths);
  if (candidates.length === 0) {
    return undefined;
  }

  return candidates
    .map((width) => `${imageProxyUrl(url, fallbackBaseUrl, { ...options, width })} ${width}w`)
    .join(", ");
};

function appendImageProxyOptions(url: string, options: ImageProxyOptions): string {
  const params = new URLSearchParams();
  if (options.format) {
    params.set("format", options.format);
  }
  if (options.width !== undefined) {
    const width = Math.round(options.width);
    if (Number.isFinite(width) && width > 0) {
      params.set("width", width.toString());
    }
  }
  const query = params.toString();
  if (!query) {
    return url;
  }

  const separator = url.includes("?") ? "&" : "?";
  return `${url}${separator}${query}`;
}

function imageProxyPath(url: string): string | null {
  const trimmedUrl = url.trim();
  if (!trimmedUrl.startsWith("/") && !/^https?:\/\//i.test(trimmedUrl)) {
    return null;
  }

  try {
    const parsed = new URL(trimmedUrl, "https://frontend.local");
    if (parsed.pathname === "/v1/media/image") {
      return `${parsed.pathname}${parsed.search}`;
    }
    if (parsed.pathname === "/api/app/media/image") {
      return `/v1/media/image${parsed.search}`;
    }
  } catch {
    return null;
  }

  return null;
}

function normalizeImageWidths(widths: readonly number[]): number[] {
  return [
    ...new Set(widths.map(Math.round).filter((width) => Number.isFinite(width) && width > 0)),
  ].sort((a, b) => a - b);
}

function normalizeAbsoluteUrl(url: string, fallbackBaseUrl?: string | null): string {
  const trimmedUrl = url.trim();

  if (trimmedUrl.startsWith("//")) {
    return `https:${trimmedUrl}`;
  }
  if (trimmedUrl.startsWith("/")) {
    return fallbackBaseUrl ? `${fallbackBaseUrl.replace(/\/$/, "")}${trimmedUrl}` : trimmedUrl;
  }
  if (!/^https?:\/\//i.test(trimmedUrl)) {
    return `https://${trimmedUrl}`;
  }

  return trimmedUrl;
}
