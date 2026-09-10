import { env } from "$env/dynamic/private";

export function privateEnv(platform: App.Platform | undefined, key: string): string {
  const binding = platform?.env?.[key];
  return typeof binding === "string" ? binding : (env[key] ?? "");
}

export function trimTrailingSlash(value: string): string {
  return value.replace(/\/+$/, "");
}

export function readJsonObject(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
