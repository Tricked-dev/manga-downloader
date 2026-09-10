import type { PageLoad } from "./$types";

interface BackendBuildInfo {
  name: string;
  version: string;
  branch: string | null;
  commit_hash: string | null;
  commit_short_hash: string | null;
  commit_date: string | null;
  build_time: string | null;
  build_channel: string | null;
  build_target: string | null;
  git_clean: boolean | null;
}

interface BackendHealthCheck {
  component: string;
  detail: string | null;
  ok: boolean;
}

interface BackendHealth {
  checks: BackendHealthCheck[];
  ok: boolean;
}

function parseBackendBuildInfo(value: unknown): BackendBuildInfo | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const decoded = value as Record<string, unknown>;
  const name = readString(decoded.name);
  const version = readString(decoded.version);
  if (!name || !version) {
    return null;
  }

  return {
    branch: readString(decoded.branch),
    build_channel: readString(decoded.build_channel),
    build_target: readString(decoded.build_target),
    build_time: readString(decoded.build_time),
    commit_date: readString(decoded.commit_date),
    commit_hash: readString(decoded.commit_hash),
    commit_short_hash: readString(decoded.commit_short_hash),
    git_clean: typeof decoded.git_clean === "boolean" ? decoded.git_clean : null,
    name,
    version,
  };
}

function parseBackendHealth(value: unknown): BackendHealth | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const decoded = value as Record<string, unknown>;
  const checks = Array.isArray(decoded.checks)
    ? decoded.checks.flatMap((check): BackendHealthCheck[] => {
        if (!check || typeof check !== "object") {
          return [];
        }

        const decodedCheck = check as Record<string, unknown>;
        const component = readString(decodedCheck.component);
        if (!component || typeof decodedCheck.ok !== "boolean") {
          return [];
        }

        return [
          {
            component,
            detail: readString(decodedCheck.detail),
            ok: decodedCheck.ok,
          },
        ];
      })
    : [];

  return {
    checks,
    ok: decoded.ok === true,
  };
}

function readString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

export const load: PageLoad = async ({ fetch, parent }) => {
  await parent();
  const [backendBuild, backendHealthy] = await Promise.all([
    fetch("/v1/info").then(async response => response.ok ? parseBackendBuildInfo(await response.json()) : null).catch(() => null),
    fetch("/v1/health").then(async response => response.ok ? parseBackendHealth(await response.json()) : null).catch(() => null),
  ]);
  return {
    application: { buildChannel: import.meta.env.MODE, buildTime: APP_BUILD_AT, commitShortHash: backendBuild?.commit_short_hash ?? null, generatedAt: APP_BUILD_AT, name: "Manga Server", version: SERVER_CARGO_VERSION },
    links: { repository: "https://github.com/Tricked-dev/manga-downloader" },
    server: {
      branch: backendBuild?.branch ?? null,
      buildChannel: backendBuild?.build_channel ?? null,
      buildTarget: backendBuild?.build_target ?? null,
      buildTime: backendBuild?.build_time ?? null,
      commitShortHash: backendBuild?.commit_short_hash ?? null,
      gitClean: backendBuild?.git_clean ?? null,
      healthChecks: backendHealthy?.checks ?? [],
      healthy: backendHealthy?.ok === true,
      isLatestCommit: null,
      lastCommitAt: backendBuild?.commit_date ?? null,
      lastCommitMessage: null,
      name: backendBuild?.name ?? "manga-server",
      remoteBranch: null,
      remoteCommitShortHash: null,
      version: backendBuild?.version ?? SERVER_CARGO_VERSION,
    },
  };
};
