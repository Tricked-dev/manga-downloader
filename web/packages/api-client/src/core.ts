import { Result, TaggedError } from "better-result";
import { getApiBase } from "./base";

export class ApiError extends TaggedError("ApiError")<{
  cause?: unknown;
  code?: string;
  details?: unknown;
  path: string;
  reason: string;
  status?: number;
}>() {}

export function isApiErrorCode(error: unknown, code: string): error is ApiError {
  return error instanceof ApiError && error.code === code;
}

export interface ApiRequestContext {
  baseUrl?: string;
  fetch?: typeof fetch;
}

export type ApiRequestOptions = RequestInit & {
  baseUrl?: string;
  fetch?: typeof globalThis.fetch;
};

const FRONTEND_MAX_ATTEMPTS = 3;
const FRONTEND_RETRY_BASE_DELAY_MS = 100;
const inFlightFrontendReads = new Map<string, Promise<unknown>>();

export function createAppApiContext(fetchFn: typeof fetch): ApiRequestContext {
  return {
    baseUrl: "/v1",
    fetch: fetchFn,
  };
}

export async function request<T = unknown>(
  path: string,
  options?: RequestInit,
  context?: ApiRequestContext,
): Promise<T> {
  return requestRaw<T>(path, options, context);
}

export async function requestRaw<T = unknown>(
  path: string,
  options?: ApiRequestOptions,
  context?: ApiRequestContext,
): Promise<T> {
  const request = prepareApiRequest(path, options, context);
  const coalesceKey = frontendCoalesceKey(request);
  if (!coalesceKey) {
    return executeApiRequest<T>(request);
  }

  const existing = inFlightFrontendReads.get(coalesceKey);
  if (existing) {
    return existing as Promise<T>;
  }

  const pending = executeApiRequest<T>(request).finally(() => {
    inFlightFrontendReads.delete(coalesceKey);
  });
  inFlightFrontendReads.set(coalesceKey, pending);

  return pending;
}

interface PreparedApiRequest {
  fetchImpl: typeof fetch;
  fetchOptions: RequestInit | undefined;
  headers: Headers;
  method: string;
  normalizedPath: string;
  url: string;
}

function prepareApiRequest(
  path: string,
  options?: ApiRequestOptions,
  context?: ApiRequestContext,
): PreparedApiRequest {
  const headers = new Headers(options?.headers);
  const body = options?.body;

  if (
    body !== undefined &&
    body !== null &&
    !headers.has("Content-Type") &&
    !(body instanceof FormData)
  ) {
    headers.set("Content-Type", "application/json");
  }

  const normalizedPath = normalizeGeneratedPath(path);
  const fetchOptions = withoutCustomOptions(options);
  const method = (fetchOptions?.method ?? "GET").toUpperCase();
  const fetchImpl = options?.fetch ?? context?.fetch ?? globalThis.fetch;
  const url = resolveApiUrl(normalizedPath, {
    ...context,
    baseUrl: options?.baseUrl ?? context?.baseUrl,
  });

  return {
    fetchImpl,
    fetchOptions,
    headers,
    method,
    normalizedPath,
    url,
  };
}

async function executeApiRequest<T>(request: PreparedApiRequest): Promise<T> {
  const maxAttempts = retryableFrontendMethod(request.method) ? FRONTEND_MAX_ATTEMPTS : 1;

  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    const responseResult = await Result.tryPromise({
      try: () =>
        request.fetchImpl(request.url, {
          ...request.fetchOptions,
          headers: request.headers,
        }),
      catch: (cause) =>
        new ApiError({ path: request.normalizedPath, reason: "Request failed", cause }),
    });

    if (responseResult.isErr()) {
      if (attempt < maxAttempts) {
        await sleep(frontendRetryDelayMs(attempt));
        continue;
      }
      throw responseResult.error;
    }

    const response = responseResult.value;
    if (!response.ok) {
      if (attempt < maxAttempts && retryableFrontendStatus(response.status)) {
        if (response.status === 502) {
          const errorPayload = await readErrorPayload(response);
          if (nonRetryableApiError(errorPayload)) {
            throw new ApiError({
              path: request.normalizedPath,
              ...errorPayload,
              status: response.status,
            });
          }
        } else {
          await response.body?.cancel().catch(() => undefined);
        }
        await sleep(frontendRetryDelayMs(attempt));
        continue;
      }

      const errorPayload = await readErrorPayload(response);
      throw new ApiError({
        path: request.normalizedPath,
        ...errorPayload,
        status: response.status,
      });
    }

    return (await readResponseBody(response)) as T;
  }

  throw new ApiError({ path: request.normalizedPath, reason: "Request failed" });
}

export function resolveApiUrl(path: string, context?: ApiRequestContext): string {
  const apiBase = context?.baseUrl?.replace(/\/$/, "") ?? getApiBase();

  if (apiBase.startsWith("http://") || apiBase.startsWith("https://")) {
    if (apiBase.endsWith("/v1")) {
      return `${apiBase}${path.startsWith("/v1") ? path.slice(3) : path}`;
    }

    return `${apiBase}${path}`;
  }

  return `${apiBase}${path.replace(/^\/v1/, "")}`;
}

function normalizeGeneratedPath(url: string): string {
  if (url.startsWith("http://") || url.startsWith("https://")) {
    const parsed = new URL(url);
    return `${parsed.pathname}${parsed.search}`;
  }

  return url.startsWith("/") ? url : `/${url}`;
}

function withoutCustomOptions(options?: ApiRequestOptions): RequestInit | undefined {
  if (!options) {
    return undefined;
  }

  const { baseUrl: _baseUrl, fetch: _fetch, ...requestOptions } = options;
  return requestOptions;
}

function frontendCoalesceKey(request: PreparedApiRequest): string | undefined {
  if (!retryableFrontendMethod(request.method) || request.fetchOptions?.body != null) {
    return undefined;
  }

  return JSON.stringify({
    headers: normalizedHeaderEntries(request.headers),
    method: request.method,
    url: request.url,
  });
}

function normalizedHeaderEntries(headers: Headers): Array<[string, string]> {
  return [...headers.entries()].sort(([left], [right]) => left.localeCompare(right));
}

function retryableFrontendMethod(method: string): boolean {
  return method === "GET" || method === "HEAD" || method === "OPTIONS";
}

function retryableFrontendStatus(status: number): boolean {
  return status === 429 || status === 502 || status === 503 || status === 504 || status >= 500;
}

function frontendRetryDelayMs(attempt: number): number {
  return FRONTEND_RETRY_BASE_DELAY_MS * 2 ** (attempt - 1);
}

function sleep(delayMs: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, delayMs);
  });
}

async function readResponseBody(response: Response): Promise<unknown> {
  const contentType = response.headers.get("content-type") ?? "";

  if (
    response.status === 204 ||
    response.status === 205 ||
    response.status === 304 ||
    !contentType.includes("application/json")
  ) {
    return undefined;
  }

  try {
    return await response.json();
  } catch (cause) {
    throw new ApiError({
      path: response.url,
      status: response.status,
      reason: "Failed to parse JSON response",
      cause,
    });
  }
}

interface ErrorPayload {
  code?: string;
  details?: unknown;
  reason: string;
}

function nonRetryableApiError(error: ErrorPayload): boolean {
  return error.code === "upstream_blocked";
}

async function readErrorPayload(response: Response): Promise<ErrorPayload> {
  const contentType = response.headers.get("content-type") ?? "";

  if (contentType.includes("application/json")) {
    const payload = (await response.json().catch(() => null)) as unknown;
    const nestedError = readNestedError(payload);
    if (nestedError) {
      return {
        code: nestedError.code,
        details: nestedError.details,
        reason: withValidationDetails(nestedError.message, nestedError.details),
      };
    }
  }

  const text = await response.text().catch(() => "");
  if (contentType.includes("text/html")) {
    return { reason: response.statusText || "Request failed" };
  }

  return { reason: text || response.statusText || "Request failed" };
}

function readNestedError(value: unknown):
  | {
      code?: string;
      details?: unknown;
      message: string;
    }
  | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }

  const error = (value as { error?: unknown }).error;
  if (!error || typeof error !== "object") {
    return undefined;
  }

  const { code, details, message } = error as {
    code?: unknown;
    details?: unknown;
    message?: unknown;
  };

  if (typeof message !== "string") {
    return undefined;
  }

  return {
    code: typeof code === "string" ? code : undefined,
    details,
    message,
  };
}

function withValidationDetails(message: string, details: unknown): string {
  const violations = readValidationViolations(details);
  if (violations.length === 0) {
    return message;
  }

  return `${message}: ${violations.join("; ")}`;
}

function readValidationViolations(details: unknown): string[] {
  if (!details || typeof details !== "object") {
    return [];
  }

  const violations = (details as { violations?: unknown }).violations;
  if (!Array.isArray(violations)) {
    return [];
  }

  return violations.flatMap((violation) => {
    if (!violation || typeof violation !== "object") {
      return [];
    }

    const { message, path } = violation as { message?: unknown; path?: unknown };
    if (typeof message !== "string" || !message.trim()) {
      return [];
    }

    return typeof path === "string" && path.trim() ? [`${path}: ${message}`] : [message];
  });
}
