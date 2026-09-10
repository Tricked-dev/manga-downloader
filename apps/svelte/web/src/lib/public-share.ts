export const PUBLIC_SHARE_QUERY_PARAM = "public";

export interface PublicShareAuthState {
  authenticated: boolean;
  enabled: boolean;
  publicAccess: PublicShareViewState | null;
  user: {
    email?: string | null;
    id?: string;
    name?: string | null;
  } | null;
}

export interface PublicShareViewState {
  pathname: string;
  routeKey: string;
  shareUrl: string;
}

export function isShareablePublicPath(pathname: string): boolean {
  const segments = pathname.split("/").filter(Boolean);
  return segments.length === 2 && segments[0] === "library" && segments[1].length > 0;
}

export function canonicalPublicShareRouteKey(url: URL): string {
  const searchParams = new URLSearchParams(url.searchParams);
  searchParams.delete(PUBLIC_SHARE_QUERY_PARAM);
  const search = searchParams.toString();

  return `${url.pathname}${search ? `?${search}` : ""}`;
}

export function publicShareUrlPath(url: URL): string {
  const sharedUrl = new URL(url);
  sharedUrl.searchParams.set(PUBLIC_SHARE_QUERY_PARAM, "true");

  return `${sharedUrl.pathname}${sharedUrl.search}${sharedUrl.hash}`;
}

export function hasPublicShareFlag(url: URL): boolean {
  return url.searchParams.get(PUBLIC_SHARE_QUERY_PARAM) === "true";
}

export function isPublicShareViewActive(
  auth: Pick<PublicShareAuthState, "authenticated" | "publicAccess"> | null | undefined,
  url: URL,
): boolean {
  return Boolean(
    auth?.authenticated === false &&
    auth.publicAccess &&
    canonicalPublicShareRouteKey(url) === auth.publicAccess.routeKey,
  );
}
