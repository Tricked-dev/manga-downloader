import { json } from "@sveltejs/kit";

const AUTH_PUBLIC_PATHS = ["/login", "/api/auth"] as const;

export function isPublicPath(pathname: string): boolean {
  return AUTH_PUBLIC_PATHS.some((path) => pathname.startsWith(path));
}

export function authDisabledSessionResponse() {
  return json({
    authEnabled: false,
    authenticated: false,
    session: null,
  });
}
