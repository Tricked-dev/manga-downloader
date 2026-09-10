import type { Handle } from "@sveltejs/kit";

import { privateEnv } from "$lib/server/env";

const ENCODER = new TextEncoder();

const SECURITY_HEADERS = {
  "cache-control": "no-store",
  "content-security-policy":
    "default-src 'self'; base-uri 'none'; connect-src 'self'; form-action 'self'; frame-ancestors 'none'; img-src 'self' data: blob:; object-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'",
  "cross-origin-opener-policy": "same-origin",
  "cross-origin-resource-policy": "same-origin",
  "permissions-policy": "camera=(), geolocation=(), microphone=(), payment=(), usb=()",
  "referrer-policy": "no-referrer",
  "x-content-type-options": "nosniff",
  "x-frame-options": "DENY",
};

export const handle: Handle = async ({ event, resolve }) => {
  const configured = isBackendConfigured(event.platform);
  const managerToken = privateEnv(event.platform, "MANAGER_ACCESS_TOKEN");

  if (configured && !managerToken) {
    return secureResponse(
      "Kanidm Manager is not available until MANAGER_ACCESS_TOKEN is configured.",
      { status: 503 },
    );
  }

  if (managerToken) {
    const requestToken = readRequestToken(event.request);
    const authorized = requestToken ? await timingSafeEqual(requestToken, managerToken) : false;

    if (!authorized) {
      return secureResponse("Authentication required.", {
        headers: {
          "www-authenticate": 'Basic realm="Kanidm Manager", charset="UTF-8"',
        },
        status: 401,
      });
    }
  }

  const response = await resolve(event);
  return secureResponse(response.body, response);
};

function isBackendConfigured(platform: App.Platform | undefined): boolean {
  return Boolean(
    privateEnv(platform, "KANIDM_BASE_URL") ||
    privateEnv(platform, "KANIDM_USERNAME") ||
    privateEnv(platform, "KANIDM_PASSWORD") ||
    privateEnv(platform, "MANGA_SERVER_BASE_URL"),
  );
}

function readRequestToken(request: Request): string {
  const header = request.headers.get("authorization")?.trim();
  if (!header) {
    return "";
  }

  const separator = header.indexOf(" ");
  if (separator === -1) {
    return "";
  }

  const scheme = header.slice(0, separator).toLowerCase();
  const value = header.slice(separator + 1).trim();
  if (scheme === "bearer") {
    return value;
  }

  if (scheme !== "basic") {
    return "";
  }

  try {
    const decoded = atob(value);
    const passwordStart = decoded.indexOf(":") + 1;
    return passwordStart > 0 ? decoded.slice(passwordStart) : decoded;
  } catch {
    return "";
  }
}

async function timingSafeEqual(candidate: string, expected: string): Promise<boolean> {
  const [candidateDigest, expectedDigest] = await Promise.all([
    crypto.subtle.digest("SHA-256", ENCODER.encode(candidate)),
    crypto.subtle.digest("SHA-256", ENCODER.encode(expected)),
  ]);

  const candidateBytes = new Uint8Array(candidateDigest);
  const expectedBytes = new Uint8Array(expectedDigest);
  let difference = candidateBytes.length ^ expectedBytes.length;

  for (let index = 0; index < candidateBytes.length; index += 1) {
    difference |= candidateBytes[index] ^ (expectedBytes[index] ?? 0);
  }

  return difference === 0;
}

function secureResponse(body: BodyInit | null, init: Response | ResponseInit): Response {
  const response = new Response(body, init);
  for (const [key, value] of Object.entries(SECURITY_HEADERS)) {
    response.headers.set(key, value);
  }
  return response;
}
