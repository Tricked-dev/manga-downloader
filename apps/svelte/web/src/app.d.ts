import type { D1Database, Fetcher } from "@cloudflare/workers-types";

// See https://svelte.dev/docs/kit/types#app.d.ts
// For information about these interfaces
declare global {
  const APP_BUILD_AT: string;
  const SERVER_CARGO_VERSION: string;

  namespace App {
    // Interface Error {}
    interface Locals {
      authEnabled: boolean;
      backendSettings?: import("@manga-server/api-client/generated/model").SettingsResponse;
      backendSettingsFresh: boolean;
      publicShare: import("$lib/server/auth/public-share").PublicShareAccess | null;
      publicShareUrl?: string;
      session: import("$lib/server/auth/config").Session | null;
      user: import("$lib/server/auth/config").Session["user"] | null;
    }
    // Interface PageData {}
    // Interface PageState {}
    interface Platform {
      env: {
        AUTH_DB?: D1Database;
        AUTH_ENABLED?: string;
        BACKEND_API_KEY?: string;
        TRASHCAN_MESH?: Fetcher;
        BACKEND_URL?: string;
        BACKEND_INTERNAL_URL?: string;
        BETTER_AUTH_SECRET?: string;
        BETTER_AUTH_URL?: string;
        OIDC_CLIENT_ID?: string;
        OIDC_CLIENT_SECRET?: string;
        OIDC_ISSUER_URL?: string;
        OIDC_PRIVATE_BASE_URL?: string;
        OIDC_PROVIDER_ID?: string;
        OIDC_SCOPES?: string;
        PUBLIC_API_BASE?: string;
      };
    }
  }
}
