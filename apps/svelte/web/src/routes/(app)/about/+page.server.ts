import { env } from "$env/dynamic/private";
import {
  applyBackendAuthorization,
  getBackendBaseUrl,
  getBackendFetch,
} from "$lib/server/api/backend-config";
import { nonEmptyStringOrNull, normalizeRepositoryUrl, shortHash } from "$lib/server/config-values";
import type { PageServerLoad } from "./$types";
import frontendPackage from "../../../../package.json";

function compareCommitRefs(
  localCommitHash: string | null,
  localCommitShortHash: string | null,
  remoteCommitHash: string | null,
  remoteCommitShortHash: string | null,
): boolean | null {
  if (localCommitHash && remoteCommitHash) {
    return localCommitHash === remoteCommitHash;
  }

  if (localCommitShortHash && remoteCommitShortHash) {
    return localCommitShortHash === remoteCommitShortHash;
  }

  if (localCommitHash && remoteCommitShortHash) {
    return shortHash(localCommitHash) === remoteCommitShortHash;
  }

  if (localCommitShortHash && remoteCommitHash) {
    return localCommitShortHash === shortHash(remoteCommitHash);
  }

  return null;
}

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

function readEnvString(...keys: string[]): string | null {
  for (const key of keys) {
    const value = nonEmptyStringOrNull(env[key]);
    if (value) {
      return value;
    }
  }

  return null;
}

function readEnvBoolean(key: string): boolean | null {
  const value = readEnvString(key)?.toLowerCase();
  if (value === "true" || value === "1") {
    return true;
  }
  if (value === "false" || value === "0") {
    return false;
  }

  return null;
}

export const load: PageServerLoad = async ({ platform }) => {
  const backendHeaders = applyBackendAuthorization(new Headers(), platform);
  const backendBaseUrl = getBackendBaseUrl(platform);
  const backendFetch = getBackendFetch(platform);
  const [backendBuild, backendHealthy] = await Promise.all([
    (async (): Promise<BackendBuildInfo | null> => {
      try {
        const response = await backendFetch(`${backendBaseUrl}/v1/info`, {
          headers: new Headers(backendHeaders),
        });
        if (!response.ok) {
          return null;
        }

        return parseBackendBuildInfo(await response.json());
      } catch {
        return null;
      }
    })(),
    (async (): Promise<BackendHealth | null> => {
      try {
        const response = await backendFetch(`${backendBaseUrl}/v1/health`, {
          headers: new Headers(backendHeaders),
        });
        if (!response.ok) {
          return null;
        }

        return parseBackendHealth(await response.json().catch(() => null));
      } catch {
        return null;
      }
    })(),
  ]);
  const branch =
    nonEmptyStringOrNull(backendBuild?.branch) ?? readEnvString("GIT_BRANCH", "CF_PAGES_BRANCH");
  const commitHash =
    nonEmptyStringOrNull(backendBuild?.commit_hash) ??
    readEnvString("GIT_COMMIT_SHA", "CF_PAGES_COMMIT_SHA");
  const commitShortHash =
    nonEmptyStringOrNull(backendBuild?.commit_short_hash) ??
    readEnvString("GIT_COMMIT_SHORT_SHA") ??
    shortHash(commitHash);
  const remoteBranch = readEnvString("GIT_REMOTE_BRANCH", "UPSTREAM_BRANCH");
  const remoteCommitHash = readEnvString("GIT_REMOTE_COMMIT_SHA", "UPSTREAM_COMMIT_SHA");
  const remoteCommitShortHash =
    readEnvString("GIT_REMOTE_COMMIT_SHORT_SHA", "UPSTREAM_COMMIT_SHORT_SHA") ??
    shortHash(remoteCommitHash);
  const repositoryUrl = normalizeRepositoryUrl(
    readEnvString("PUBLIC_REPOSITORY_URL", "REPOSITORY_URL") ??
      "https://github.com/TrashCan69420/manga-downloader",
  );
  const frontendBuildChannel =
    readEnvString("DEPLOY_ENV", "CLOUDFLARE_ENV", "NODE_ENV", "MODE") ?? "production";

  return {
    application: {
      buildChannel: frontendBuildChannel,
      buildTime: readEnvString("GIT_COMMIT_AT") ?? APP_BUILD_AT,
      commitShortHash,
      generatedAt: APP_BUILD_AT,
      name: "Manga Server",
      version: frontendPackage.version ?? "unknown",
    },
    links: {
      repository: repositoryUrl,
    },
    server: {
      branch,
      buildChannel: backendBuild?.build_channel ?? null,
      buildTarget: backendBuild?.build_target ?? null,
      buildTime: backendBuild?.build_time ?? null,
      commitShortHash,
      gitClean: backendBuild?.git_clean ?? readEnvBoolean("GIT_CLEAN"),
      healthChecks: backendHealthy?.checks ?? [],
      healthy: backendHealthy?.ok === true || backendBuild !== null,
      isLatestCommit: compareCommitRefs(
        commitHash,
        commitShortHash,
        remoteCommitHash,
        remoteCommitShortHash,
      ),
      lastCommitAt:
        nonEmptyStringOrNull(backendBuild?.commit_date) ?? readEnvString("GIT_COMMIT_AT"),
      lastCommitMessage: readEnvString("GIT_COMMIT_MESSAGE"),
      name: backendBuild?.name ?? "manga-server",
      remoteBranch,
      remoteCommitShortHash,
      version: backendBuild?.version ?? SERVER_CARGO_VERSION,
    },
  };
};
