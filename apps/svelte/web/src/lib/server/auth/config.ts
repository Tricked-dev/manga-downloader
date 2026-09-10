import { env } from "$env/dynamic/private";
import {
  applyBackendAuthorization,
  getBackendBaseUrl,
  getBackendFetch,
  getPrivateNetworkFetch,
} from "$lib/server/api/backend-config";
import { absoluteUrlOrNull, firstDefined, nonEmptyStringOrNull } from "$lib/server/config-values";
import type { SettingsResponse } from "@manga-server/api-client/generated/model";
import { getAuthDatabase } from "./storage";

interface RuntimeAuthSettings {
  auth_enabled: string;
  auth_oidc_issuer_url: string;
  auth_oidc_client_id: string;
  auth_oidc_client_secret: string;
  auth_oidc_scopes: string;
  auth_oidc_provider_id: string;
}

interface AuthContext {
  auth: AuthInstance | null;
  authEnabled: boolean;
  oidcConfigured: boolean;
  oidcProviderId: string;
  settingsResponse: SettingsResponse;
  settingsResponseFresh: boolean;
}

const AUTH_SETTINGS_CACHE_TTL_MS = 5000;

const DEFAULT_RUNTIME_AUTH_SETTINGS: RuntimeAuthSettings = {
  auth_enabled: "false",
  auth_oidc_client_id: "",
  auth_oidc_client_secret: "",
  auth_oidc_issuer_url: "",
  auth_oidc_provider_id: "oidc",
  auth_oidc_scopes: "openid profile email",
};

type AuthInstance = Awaited<ReturnType<typeof buildAuth>>;
export type Session = NonNullable<Awaited<ReturnType<AuthInstance["api"]["getSession"]>>>;
type OAuthFetch = (input: string, init?: RequestInit) => Promise<OAuthResponse>;

interface OAuthResponse {
  json(): Promise<unknown>;
  ok: boolean;
  status: number;
  statusText: string;
}

interface OAuth2TokenPayload {
  access_token?: string;
  expires_in?: number;
  id_token?: string;
  refresh_token?: string;
  refresh_token_expires_in?: number;
  scope?: string | string[];
  token_type?: string;
}

interface OAuth2Tokens {
  accessToken?: string;
  accessTokenExpiresAt?: Date;
  idToken?: string;
  raw?: Record<string, unknown>;
  refreshToken?: string;
  refreshTokenExpiresAt?: Date;
  scopes: string[];
  tokenType?: string;
}

interface OAuth2UserInfo {
  email?: string;
  emailVerified: boolean;
  id: string;
  image?: string;
  name?: string;
  [key: string]: unknown;
}

let authSettingsCache: {
  expiresAt: number;
  value: RuntimeAuthSettingsSnapshot;
} | null = null;
let pendingAuthSettingsLoad: Promise<RuntimeAuthSettingsSnapshot> | null = null;
let authStorageReady: Promise<void> | null = null;

interface RuntimeAuthSettingsSnapshot {
  settings: RuntimeAuthSettings;
  settingsResponse: SettingsResponse;
}

export async function getAuthContext(
  request?: Request,
  platform?: App.Platform,
): Promise<AuthContext> {
  const { settings, settingsResponse, settingsResponseFresh } =
    await getRuntimeAuthSettings(platform);
  const oidcIssuerUrl = settings.auth_oidc_issuer_url.trim();
  const oidcClientId = settings.auth_oidc_client_id.trim();
  const oidcClientSecret = settings.auth_oidc_client_secret.trim();
  const oidcConfigured = Boolean(oidcIssuerUrl && oidcClientId && oidcClientSecret);
  const oidcProviderId =
    settings.auth_oidc_provider_id.trim() || DEFAULT_RUNTIME_AUTH_SETTINGS.auth_oidc_provider_id;
  const authEnabled = settings.auth_enabled === "true" && oidcConfigured;
  const requestOrigin = resolveRequestOrigin(request);

  return {
    auth: authEnabled
      ? await buildAuth({
          oidcClientId,
          oidcClientSecret,
          oidcConfigured,
          oidcIssuerUrl,
          oidcProviderId,
          oidcScopes: parseScopes(settings.auth_oidc_scopes),
          platform,
          requestOrigin,
        })
      : null,
    authEnabled,
    oidcConfigured,
    oidcProviderId,
    settingsResponse,
    settingsResponseFresh,
  };
}

export function invalidateAuthSettingsCache(): void {
  authSettingsCache = null;
  pendingAuthSettingsLoad = null;
}

export async function ensureAuthStorageReady(
  authEnabled: boolean,
  auth: AuthInstance | null,
): Promise<void> {
  if (!authEnabled || !auth) {
    return;
  }

  authStorageReady ??= (async () => {
    const context = await auth.$context;
    await context.runMigrations();
  })().catch((error: unknown) => {
    authStorageReady = null;
    throw error;
  });

  return authStorageReady ?? Promise.resolve();
}

async function buildAuth({
  oidcClientId,
  oidcClientSecret,
  oidcConfigured,
  oidcIssuerUrl,
  oidcProviderId,
  oidcScopes,
  platform,
  requestOrigin,
}: {
  oidcClientId: string;
  oidcClientSecret: string;
  oidcConfigured: boolean;
  oidcIssuerUrl: string;
  oidcProviderId: string;
  oidcScopes: string[];
  platform?: App.Platform;
  requestOrigin?: string;
}) {
  const [{ betterAuth }, { genericOAuth }] = await Promise.all([
    import("better-auth"),
    import("better-auth/plugins/generic-oauth"),
  ]);
  const database = await getAuthDatabase(platform);
  const configuredBaseUrl =
    absoluteUrlOrNull(env.BETTER_AUTH_URL) ?? absoluteUrlOrNull(platform?.env.BETTER_AUTH_URL);
  const baseURL = configuredBaseUrl ?? requestOrigin;
  const trustedOrigins = [configuredBaseUrl, requestOrigin].filter((value): value is string =>
    Boolean(value),
  );
  const oidcPrivateBaseUrl =
    absoluteUrlOrNull(env.OIDC_PRIVATE_BASE_URL) ??
    absoluteUrlOrNull(platform?.env.OIDC_PRIVATE_BASE_URL) ??
    undefined;
  const oidcEndpoints = resolveKanidmOidcEndpoints(oidcIssuerUrl, oidcPrivateBaseUrl);
  const privateFetch = getPrivateNetworkFetch(platform);
  const oidcFetch: OAuthFetch = privateFetch
    ? (input, init) => privateFetch(input, init) as Promise<OAuthResponse>
    : (input, init) => fetch(input, init) as Promise<OAuthResponse>;

  return betterAuth({
    appName: "Manga Server",
    baseURL,
    database,
    emailAndPassword: {
      enabled: false,
    },
    plugins: oidcConfigured
      ? [
          genericOAuth({
            config: [
              {
                authentication: "basic",
                authorizationUrl: oidcEndpoints.authorizationUrl,
                clientId: oidcClientId,
                clientSecret: oidcClientSecret,
                getToken: ({ code, codeVerifier, redirectURI }) =>
                  exchangeKanidmAuthorizationCode({
                    clientId: oidcClientId,
                    clientSecret: oidcClientSecret,
                    code,
                    codeVerifier,
                    fetch: oidcFetch,
                    redirectURI,
                    tokenUrl: oidcEndpoints.tokenUrl,
                  }),
                getUserInfo: (tokens) =>
                  getKanidmUserInfo(tokens, oidcEndpoints.userInfoUrl, oidcFetch),
                issuer: oidcIssuerUrl,
                pkce: true,
                providerId: oidcProviderId,
                scopes: oidcScopes,
                tokenUrl: oidcEndpoints.tokenUrl,
                userInfoUrl: oidcEndpoints.userInfoUrl,
              },
            ],
          }),
        ]
      : [],
    secret: env.BETTER_AUTH_SECRET ?? platform?.env.BETTER_AUTH_SECRET ?? undefined,
    session: {
      storeSessionInDatabase: true,
    },
    trustedOrigins: trustedOrigins.length > 0 ? [...new Set(trustedOrigins)] : undefined,
  });
}

async function getRuntimeAuthSettings(platform?: App.Platform): Promise<
  RuntimeAuthSettingsSnapshot & {
    settingsResponseFresh: boolean;
  }
> {
  if (authSettingsCache && authSettingsCache.expiresAt > Date.now()) {
    return {
      ...authSettingsCache.value,
      settingsResponseFresh: false,
    };
  }

  pendingAuthSettingsLoad ??= loadRuntimeAuthSettings(platform).finally(() => {
    pendingAuthSettingsLoad = null;
  });

  const settings = await pendingAuthSettingsLoad;
  authSettingsCache = {
    expiresAt: Date.now() + AUTH_SETTINGS_CACHE_TTL_MS,
    value: settings,
  };
  return {
    ...settings,
    settingsResponseFresh: true,
  };
}

async function loadRuntimeAuthSettings(
  platform?: App.Platform,
): Promise<RuntimeAuthSettingsSnapshot> {
  try {
    const response = await getBackendFetch(platform)(`${getBackendBaseUrl(platform)}/v1/settings`, {
      headers: applyBackendAuthorization(new Headers(), platform),
    });
    if (!response.ok) {
      const settingsResponse = createSettingsResponse({});
      return {
        settings: normalizeRuntimeAuthSettings(settingsResponse.settings, platform),
        settingsResponse,
      };
    }

    const settingsResponse = createSettingsResponse(
      readSettingsResponse(await response.json().catch(() => null)),
    );
    return {
      settings: normalizeRuntimeAuthSettings(settingsResponse.settings, platform),
      settingsResponse,
    };
  } catch {
    const settingsResponse = createSettingsResponse({});
    return {
      settings: normalizeRuntimeAuthSettings(settingsResponse.settings, platform),
      settingsResponse,
    };
  }
}

function createSettingsResponse(settings: Record<string, string>): SettingsResponse {
  return { settings };
}

function readSettingsResponse(value: unknown): Record<string, string> {
  if (!value || typeof value !== "object") {
    return {};
  }

  const settings = (value as { settings?: unknown }).settings;
  if (!settings || typeof settings !== "object") {
    return {};
  }

  return Object.fromEntries(
    Object.entries(settings as Record<string, unknown>).flatMap(([key, entry]) =>
      typeof entry === "string" ? [[key, entry]] : [],
    ),
  );
}

function normalizeRuntimeAuthSettings(
  settings: Record<string, string>,
  platform?: App.Platform,
): RuntimeAuthSettings {
  return {
    auth_enabled: firstDefined(
      settings.auth_enabled,
      env.AUTH_ENABLED,
      platform?.env.AUTH_ENABLED,
      DEFAULT_RUNTIME_AUTH_SETTINGS.auth_enabled,
    ),
    auth_oidc_client_id: firstDefined(
      settings.auth_oidc_client_id,
      env.OIDC_CLIENT_ID,
      platform?.env.OIDC_CLIENT_ID,
      DEFAULT_RUNTIME_AUTH_SETTINGS.auth_oidc_client_id,
    ),
    auth_oidc_client_secret: firstDefined(
      nonEmptyStringOrNull(settings.auth_oidc_client_secret) ?? undefined,
      env.OIDC_CLIENT_SECRET,
      platform?.env.OIDC_CLIENT_SECRET,
      DEFAULT_RUNTIME_AUTH_SETTINGS.auth_oidc_client_secret,
    ),
    auth_oidc_issuer_url: firstDefined(
      settings.auth_oidc_issuer_url,
      env.OIDC_ISSUER_URL,
      platform?.env.OIDC_ISSUER_URL,
      DEFAULT_RUNTIME_AUTH_SETTINGS.auth_oidc_issuer_url,
    ),
    auth_oidc_provider_id: firstDefined(
      settings.auth_oidc_provider_id,
      env.OIDC_PROVIDER_ID,
      platform?.env.OIDC_PROVIDER_ID,
      DEFAULT_RUNTIME_AUTH_SETTINGS.auth_oidc_provider_id,
    ),
    auth_oidc_scopes: firstDefined(
      settings.auth_oidc_scopes,
      env.OIDC_SCOPES,
      platform?.env.OIDC_SCOPES,
      DEFAULT_RUNTIME_AUTH_SETTINGS.auth_oidc_scopes,
    ),
  };
}

function resolveRequestOrigin(request?: Request): string | undefined {
  if (!request) {
    return undefined;
  }

  try {
    return new URL(request.url).origin;
  } catch {
    return undefined;
  }
}

function parseScopes(value: string): string[] {
  const scopes = value
    .split(/\s+/)
    .map((scope) => scope.trim())
    .filter(Boolean);

  return scopes.length > 0 ? scopes : DEFAULT_RUNTIME_AUTH_SETTINGS.auth_oidc_scopes.split(/\s+/);
}

function resolveKanidmOidcEndpoints(
  issuerUrl: string,
  privateBaseUrl?: string,
): {
  authorizationUrl: string;
  tokenUrl: string;
  userInfoUrl: string;
} {
  const issuer = new URL(issuerUrl.replace(/\/$/, ""));
  const tokenBase = privateBaseUrl ? new URL(privateBaseUrl.replace(/\/$/, "")) : issuer;
  return {
    authorizationUrl: `${issuer.origin}/ui/oauth2`,
    tokenUrl: `${tokenBase.origin}/oauth2/token`,
    userInfoUrl: `${tokenBase.origin}${issuer.pathname}/userinfo`,
  };
}

async function exchangeKanidmAuthorizationCode({
  clientId,
  clientSecret,
  code,
  codeVerifier,
  fetch,
  redirectURI,
  tokenUrl,
}: {
  clientId: string;
  clientSecret: string;
  code: string;
  codeVerifier?: string;
  fetch: OAuthFetch;
  redirectURI: string;
  tokenUrl: string;
}): Promise<OAuth2Tokens> {
  const body = new URLSearchParams({
    code,
    grant_type: "authorization_code",
    redirect_uri: redirectURI,
  });
  if (codeVerifier) {
    body.set("code_verifier", codeVerifier);
  }

  const response = await fetch(tokenUrl, {
    body,
    headers: {
      accept: "application/json",
      authorization: `Basic ${base64Encode(`${clientId}:${clientSecret}`)}`,
      "content-type": "application/x-www-form-urlencoded",
    },
    method: "POST",
  });

  return mapOAuth2Tokens(await readOAuthJson(response));
}

async function getKanidmUserInfo(
  tokens: { accessToken?: string },
  userInfoUrl: string,
  fetch: OAuthFetch,
): Promise<OAuth2UserInfo | null> {
  if (!tokens.accessToken) {
    return null;
  }

  const response = await fetch(userInfoUrl, {
    headers: {
      accept: "application/json",
      authorization: `Bearer ${tokens.accessToken}`,
    },
    method: "GET",
  });
  const userInfo = await readOAuthJson(response);
  const subject = typeof userInfo.sub === "string" ? userInfo.sub : "";
  const email = typeof userInfo.email === "string" ? userInfo.email : undefined;
  const emailVerified =
    typeof userInfo.email_verified === "boolean" ? userInfo.email_verified : false;
  const image = typeof userInfo.picture === "string" ? userInfo.picture : undefined;
  const name = typeof userInfo.name === "string" ? userInfo.name : undefined;

  return {
    ...userInfo,
    email,
    emailVerified,
    id: subject,
    image,
    name,
  };
}

async function readOAuthJson(response: OAuthResponse): Promise<Record<string, unknown>> {
  const data = await response.json().catch(() => null);
  if (!response.ok) {
    throw {
      status: response.status,
      statusText: response.statusText,
    };
  }
  if (!data || typeof data !== "object") {
    throw new Error("OAuth endpoint returned a non-object response");
  }
  return data as Record<string, unknown>;
}

function mapOAuth2Tokens(data: Record<string, unknown>): OAuth2Tokens {
  const payload = data as OAuth2TokenPayload;
  return {
    accessToken: typeof payload.access_token === "string" ? payload.access_token : undefined,
    accessTokenExpiresAt:
      typeof payload.expires_in === "number" ? secondsFromNow(payload.expires_in) : undefined,
    idToken: typeof payload.id_token === "string" ? payload.id_token : undefined,
    raw: data,
    refreshToken: typeof payload.refresh_token === "string" ? payload.refresh_token : undefined,
    refreshTokenExpiresAt:
      typeof payload.refresh_token_expires_in === "number"
        ? secondsFromNow(payload.refresh_token_expires_in)
        : undefined,
    scopes:
      typeof payload.scope === "string"
        ? payload.scope.split(/\s+/).filter(Boolean)
        : Array.isArray(payload.scope)
          ? payload.scope.filter((scope): scope is string => typeof scope === "string")
          : [],
    tokenType: typeof payload.token_type === "string" ? payload.token_type : undefined,
  };
}

function secondsFromNow(seconds: number): Date {
  return new Date(Date.now() + seconds * 1000);
}

function base64Encode(value: string): string {
  const bytes = new TextEncoder().encode(value);
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}
