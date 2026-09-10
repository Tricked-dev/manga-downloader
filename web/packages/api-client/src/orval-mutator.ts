import type { ApiRequestContext } from "./core";
import { requestRaw } from "./core";

export type CustomFetchOptions = RequestInit & {
  baseUrl?: string;
  fetch?: typeof globalThis.fetch;
};

export async function customFetch<T>(url: string, options?: CustomFetchOptions): Promise<T> {
  return requestRaw<T>(url, options);
}

export function customFetchOptions(context?: ApiRequestContext): CustomFetchOptions | undefined {
  return context?.baseUrl || context?.fetch
    ? { baseUrl: context.baseUrl, fetch: context.fetch }
    : undefined;
}
