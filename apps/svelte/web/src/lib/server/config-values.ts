export function nonEmptyStringOrNull(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }

  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export function absoluteUrlOrNull(value: unknown): string | null {
  const trimmed = nonEmptyStringOrNull(value)?.replace(/\/$/, "");
  return trimmed && /^https?:\/\//i.test(trimmed) ? trimmed : null;
}

export function firstDefined(...values: (string | undefined)[]): string {
  for (const value of values) {
    if (value !== undefined) {
      return value;
    }
  }

  return "";
}

export function shortHash(value: string | null | undefined): string | null {
  return nonEmptyStringOrNull(value)?.slice(0, 7) ?? null;
}

export function normalizeRepositoryUrl(remoteUrl: string | null | undefined): string | null {
  const normalized = nonEmptyStringOrNull(remoteUrl);
  if (!normalized) {
    return null;
  }

  if (/^https?:\/\//i.test(normalized)) {
    return normalized.replace(/\.git$/i, "");
  }

  const sshMatch = /^git@([^:]+):(.+?)(?:\.git)?$/i.exec(normalized);
  if (sshMatch) {
    return `https://${sshMatch[1]}/${sshMatch[2]}`;
  }

  const sshProtocolMatch = /^ssh:\/\/git@([^/]+)\/(.+?)(?:\.git)?$/i.exec(normalized);
  if (sshProtocolMatch) {
    return `https://${sshProtocolMatch[1]}/${sshProtocolMatch[2]}`;
  }

  return null;
}
