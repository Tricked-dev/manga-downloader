import { getAuthContext } from "$lib/server/auth/config";
import {
  createPublicShareForUrl,
  deletePublicShareForUrl,
  findPublicShareForUrl,
  publicShareUrlForRecord,
} from "$lib/server/auth/public-share";
import { error, json } from "@sveltejs/kit";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async ({ locals, platform, request, url }) => {
  await ensurePublicShareSession({ locals, platform, request });
  const shareUrl = readShareUrlParam(url);
  const share = await findPublicShareForUrl(shareUrl, platform);

  return json({
    public: Boolean(share),
    shareUrl: share ? publicShareUrlForRecord(share) : null,
  });
};

export const POST: RequestHandler = async ({ locals, platform, request }) => {
  await ensurePublicShareSession({ locals, platform, request });
  const shareUrl = await readShareUrlBody(request);
  const share = await createPublicShareForUrl({
    platform,
    url: shareUrl,
    user: locals.user!,
  });

  return json({
    public: true,
    shareUrl: publicShareUrlForRecord(share),
  });
};

export const DELETE: RequestHandler = async ({ locals, platform, request, url }) => {
  await ensurePublicShareSession({ locals, platform, request });
  const shareUrl = readShareUrlParam(url);
  const deleted = await deletePublicShareForUrl(shareUrl, platform);

  return json({
    deleted,
    public: false,
  });
};

async function ensurePublicShareSession({
  locals,
  platform,
  request,
}: {
  locals: App.Locals;
  platform?: App.Platform;
  request: Request;
}): Promise<void> {
  const { authEnabled } = await getAuthContext(request, platform);

  if (!authEnabled) {
    throw error(400, "Authentication is disabled");
  }
  if (!locals.session || !locals.user) {
    throw error(401, "Authentication required");
  }
}

function readShareUrlParam(url: URL): URL {
  const value = url.searchParams.get("url");
  if (!value) {
    throw error(400, "Missing url");
  }

  return parseSameOriginUrl(value, url.origin);
}

async function readShareUrlBody(request: Request): Promise<URL> {
  const body = (await request.json().catch(() => null)) as { url?: unknown } | null;
  if (!body || typeof body.url !== "string") {
    throw error(400, "Missing url");
  }

  return parseSameOriginUrl(body.url, new URL(request.url).origin);
}

function parseSameOriginUrl(value: string, origin: string): URL {
  try {
    const parsed = new URL(value, origin);
    if (parsed.origin !== origin) {
      throw error(400, "Only same-origin URLs can be shared");
    }

    return parsed;
  } catch (caught) {
    if (caught && typeof caught === "object" && "status" in caught) {
      throw caught;
    }
    throw error(400, "Invalid url");
  }
}
