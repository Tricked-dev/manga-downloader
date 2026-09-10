export interface ApiBaseOptions {
  baseUrl?: string;
}

export interface ApiClientConfig {
  browserBaseUrl?: string;
  publicApiBase?: string;
  serverFallbackBaseUrl?: string;
}

let config: ApiClientConfig = {};

export function configureApiClient(nextConfig: ApiClientConfig): void {
  config = {
    ...config,
    ...nextConfig,
  };
  API_BASE = getApiBase();
}

export function getApiBase(options?: ApiBaseOptions): string {
  if (options?.baseUrl) {
    return options.baseUrl.replace(/\/$/, "");
  }

  if (globalThis.window !== undefined) {
    return config.browserBaseUrl ?? "/api/app";
  }

  const publicApiBase = config.publicApiBase ?? readPublicApiBaseFromProcess();
  if (publicApiBase) {
    const configuredBase = publicApiBase.replace(/\/$/, "");
    if (configuredBase.startsWith("http://") || configuredBase.startsWith("https://")) {
      return configuredBase;
    }
  }

  return config.serverFallbackBaseUrl?.replace(/\/$/, "") ?? "http://localhost:4000";
}

export let API_BASE = getApiBase();

export function getBrowserApiBase(options?: ApiBaseOptions): string {
  return (options?.baseUrl ?? config.browserBaseUrl ?? "/api/app").replace(/\/$/, "");
}

function readPublicApiBaseFromProcess(): string | undefined {
  const processLike = (globalThis as { process?: { env?: Record<string, string | undefined> } })
    .process;
  return processLike?.env?.PUBLIC_API_BASE;
}
