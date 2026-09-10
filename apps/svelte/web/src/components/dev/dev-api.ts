import { resolve } from "$app/paths";

type Translate = (key: string, options?: Record<string, unknown>) => string;

export interface SessionUser {
  email?: string | null;
  id?: string;
  name?: string | null;
  role?: string | null;
}

export interface SessionResponse {
  authEnabled: boolean;
  authenticated: boolean;
  session: {
    user?: SessionUser | null;
  } | null;
}

export interface HealthResponse {
  checks?: Array<{ component?: string; detail?: string | null; ok?: boolean }>;
  ok?: boolean;
}

export interface InfoResponse {
  branch?: string | null;
  build_channel?: string | null;
  build_target?: string | null;
  build_time?: string | null;
  commit_short_hash?: string | null;
  git_clean?: boolean | null;
  name?: string;
  version?: string;
}

export interface ListResponse<T> {
  items?: T[];
}

export interface SettingsResponse {
  settings?: Record<string, string>;
}

export interface DownloadRow {
  status?: string;
}

export interface SourceInfo {
  enabled?: boolean;
}

export interface AdminSummary {
  downloads: DownloadRow[];
  health: HealthResponse | null;
  info: InfoResponse | null;
  libraryCount: number;
  settings: Record<string, string>;
  sources: SourceInfo[];
}

export async function loadDevSession(
  signal: AbortSignal | undefined,
  translate: Translate,
): Promise<SessionResponse> {
  const response = await fetch(resolve("/api/app/session"), { signal });
  if (!response.ok) {
    throw new Error(translate("app.dev.toolbar.sessionRequestFailed", { status: response.status }));
  }

  return (await response.json()) as SessionResponse;
}

export async function signOutDevSession(): Promise<boolean> {
  const response = await fetch(resolve("/api/app/auth/signout"), {
    method: "POST",
  });

  return response.ok;
}

export async function loadDevAdminSummary(
  signal: AbortSignal | undefined,
  translate: Translate,
): Promise<{
  errorMessage: string | null;
  summary: AdminSummary;
}> {
  const [health, info, settings, library, downloads, sources] = await Promise.allSettled([
    fetchJson<HealthResponse>("/api/app/health", signal),
    fetchJson<InfoResponse>("/api/app/info", signal),
    fetchJson<SettingsResponse>("/api/app/settings", signal),
    fetchJson<ListResponse<unknown>>("/api/app/library", signal),
    fetchJson<ListResponse<DownloadRow>>("/api/app/downloads", signal),
    fetchJson<ListResponse<SourceInfo>>("/api/app/sources", signal),
  ]);

  const failedRequest = [health, info, settings, library, downloads, sources].find(
    (result) => result.status === "rejected",
  );

  return {
    errorMessage:
      failedRequest?.status === "rejected"
        ? failedRequest.reason instanceof Error
          ? failedRequest.reason.message
          : translate("app.dev.admin.requestsFailed")
        : null,
    summary: {
      downloads: settledValue(downloads)?.items ?? [],
      health: settledValue(health),
      info: settledValue(info),
      libraryCount: settledValue(library)?.items?.length ?? 0,
      settings: settledValue(settings)?.settings ?? {},
      sources: settledValue(sources)?.items ?? [],
    },
  };
}

async function fetchJson<T>(path: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(path, { signal });
  if (!response.ok) {
    throw new Error(`${path} returned ${response.status}`);
  }

  return (await response.json()) as T;
}

function settledValue<T>(result: PromiseSettledResult<T>): T | null {
  return result.status === "fulfilled" ? result.value : null;
}
