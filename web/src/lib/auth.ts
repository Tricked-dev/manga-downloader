export interface BrowserSession {
  authEnabled: boolean;
  oidcConfigured: boolean;
  user: { id: string; name: string | null; email: string | null } | null;
  publicAccess: { pathname: string; routeKey: string; url: string } | null;
}

export async function readSession(fetcher: typeof fetch = fetch): Promise<BrowserSession> {
  const response = await fetcher("/auth/session", { cache: "no-store" });
  if (!response.ok) throw new Error("Unable to check your sign-in session.");
  return response.json() as Promise<BrowserSession>;
}
