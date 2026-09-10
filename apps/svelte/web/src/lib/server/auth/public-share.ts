import {
  canonicalPublicShareRouteKey,
  hasPublicShareFlag,
  isShareablePublicPath,
  publicShareUrlPath,
} from "$lib/public-share";
import { getAuthDatabase } from "./storage";
import type { D1Database } from "@cloudflare/workers-types";
import type { Session } from "./config";

export const PUBLIC_SHARE_COOKIE_NAME = "manga_public_share";
export const PUBLIC_SHARE_COOKIE_MAX_AGE = 60 * 60 * 12;

interface PublicShareRow {
  created_at: string;
  created_by_email: string | null;
  created_by_user_id: string | null;
  id: string;
  last_accessed_at: string | null;
  pathname: string;
  route_key: string;
  search: string;
  updated_at: string;
}

export interface PublicShareRecord {
  createdAt: string;
  createdByEmail: string | null;
  createdByUserId: string | null;
  id: string;
  lastAccessedAt: string | null;
  pathname: string;
  routeKey: string;
  search: string;
  updatedAt: string;
}

export interface PublicShareAccess {
  share: PublicShareRecord;
}

type AuthDatabase = Awaited<ReturnType<typeof getAuthDatabase>>;
type LocalAuthDatabase = import("better-sqlite3").Database;

const PUBLIC_SHARE_SCHEMA = `
  CREATE TABLE IF NOT EXISTS public_shares (
    id TEXT PRIMARY KEY,
    route_key TEXT NOT NULL UNIQUE,
    pathname TEXT NOT NULL,
    search TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_accessed_at TEXT,
    created_by_user_id TEXT,
    created_by_email TEXT
  )
`;

const storageReady = new WeakMap<object, Promise<void>>();

export function publicShareUrlForRecord(share: PublicShareRecord): string {
  const url = new URL(`http://manga.local${share.routeKey}`);
  return publicShareUrlPath(url);
}

export function publicShareRequestHeaders(
  access: PublicShareAccess | null,
  existingCookieHeader?: string | null,
): HeadersInit | undefined {
  if (!access) {
    return undefined;
  }

  const headers = new Headers();
  headers.set(
    "cookie",
    mergeCookieHeader(existingCookieHeader, `${PUBLIC_SHARE_COOKIE_NAME}=${access.share.id}`),
  );

  return headers;
}

export async function findPublicShareForUrl(
  url: URL,
  platform?: App.Platform,
): Promise<PublicShareRecord | null> {
  if (!isShareablePublicPath(url.pathname)) {
    return null;
  }

  const routeKey = canonicalPublicShareRouteKey(url);
  const db = await getPublicShareDatabase(platform);
  return findPublicShareByRouteKey(db, routeKey);
}

export async function createPublicShareForUrl({
  platform,
  url,
  user,
}: {
  platform?: App.Platform;
  url: URL;
  user: Session["user"];
}): Promise<PublicShareRecord> {
  if (!isShareablePublicPath(url.pathname)) {
    throw new Error("This route cannot be shared publicly");
  }

  const db = await getPublicShareDatabase(platform);
  const routeKey = canonicalPublicShareRouteKey(url);
  const now = new Date().toISOString();
  const existing = await findPublicShareByRouteKey(db, routeKey);

  if (existing) {
    await runStatement(db, "UPDATE public_shares SET updated_at = ? WHERE id = ?", [
      now,
      existing.id,
    ]);
    return {
      ...existing,
      updatedAt: now,
    };
  }

  const id = crypto.randomUUID();
  const search = new URL(`http://manga.local${routeKey}`).search;
  await runStatement(
    db,
    `INSERT INTO public_shares (
      id,
      route_key,
      pathname,
      search,
      created_at,
      updated_at,
      created_by_user_id,
      created_by_email
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
    [id, routeKey, url.pathname, search, now, now, user.id ?? null, user.email ?? null],
  );

  const created = await findPublicShareByRouteKey(db, routeKey);
  if (!created) {
    throw new Error("Public share was not persisted");
  }

  return created;
}

export async function deletePublicShareForUrl(url: URL, platform?: App.Platform): Promise<boolean> {
  const db = await getPublicShareDatabase(platform);
  const routeKey = canonicalPublicShareRouteKey(url);
  const result = await runStatement(db, "DELETE FROM public_shares WHERE route_key = ?", [
    routeKey,
  ]);

  return result.changes > 0;
}

export async function resolvePublicShareAccess({
  method,
  platform,
  shareId,
  url,
  user,
}: {
  method?: string;
  platform?: App.Platform;
  shareId?: string;
  url: URL;
  user: Session["user"] | null;
}): Promise<PublicShareAccess | null> {
  if (hasPublicShareFlag(url) && isShareablePublicPath(url.pathname)) {
    const share = user
      ? await createPublicShareForUrl({ platform, url, user })
      : await findPublicShareForUrl(url, platform);

    if (share) {
      await touchPublicShare(share.id, platform);
      return { share };
    }
  }

  if (!shareId) {
    return null;
  }

  const share = await findPublicShareById(shareId, platform);
  const access = share ? { share } : null;
  if (
    !access ||
    (!isPublicShareRequestAllowed(access, url) &&
      !isPublicShareApiRequestAllowed(access, url.pathname, method ?? "GET"))
  ) {
    return null;
  }

  await touchPublicShare(access.share.id, platform);
  return access;
}

export function isPublicShareRequestAllowed(access: PublicShareAccess | null, url: URL): boolean {
  if (!access) {
    return false;
  }

  if (!isShareablePublicPath(access.share.pathname)) {
    return false;
  }

  if (canonicalPublicShareRouteKey(url) === access.share.routeKey) {
    return true;
  }

  return isPublicShareReaderRequestAllowed(access, url);
}

export function isPublicShareApiRequestAllowed(
  access: PublicShareAccess | null,
  urlOrPathname: URL | string,
  method: string,
): boolean {
  if (!access || !["GET", "HEAD"].includes(method.toUpperCase())) {
    return false;
  }

  const sharePath = access.share.pathname;
  if (!isShareablePublicPath(sharePath)) {
    return false;
  }

  const url =
    typeof urlOrPathname === "string"
      ? new URL(urlOrPathname, "http://manga.local")
      : urlOrPathname;
  const pathname = url.pathname;
  if (pathname === "/api/app/media/image") {
    return true;
  }

  const shareSegments = pathSegments(sharePath);
  if (sharePath.startsWith("/library/")) {
    const seriesId = shareSegments[1];
    return (
      pathname === `/api/app/library/${seriesId}` ||
      pathname === `/api/app/library/${seriesId}/chapters` ||
      isPublicShareDownloadedChapterApiRequestAllowed(seriesId, url)
    );
  }

  return false;
}

async function findPublicShareById(
  id: string,
  platform?: App.Platform,
): Promise<PublicShareRecord | null> {
  const db = await getPublicShareDatabase(platform);
  return findPublicShareBy(db, "id", id);
}

async function touchPublicShare(id: string, platform?: App.Platform): Promise<void> {
  const db = await getPublicShareDatabase(platform);
  await runStatement(db, "UPDATE public_shares SET last_accessed_at = ? WHERE id = ?", [
    new Date().toISOString(),
    id,
  ]);
}

async function getPublicShareDatabase(platform?: App.Platform): Promise<AuthDatabase> {
  const db = await getAuthDatabase(platform);
  let ready = storageReady.get(db as object);
  if (!ready) {
    ready = runStatement(db, PUBLIC_SHARE_SCHEMA, []).then(() => undefined);
    storageReady.set(db as object, ready);
  }
  await ready;
  return db;
}

async function findPublicShareByRouteKey(
  db: AuthDatabase,
  routeKey: string,
): Promise<PublicShareRecord | null> {
  return findPublicShareBy(db, "route_key", routeKey);
}

async function findPublicShareBy(
  db: AuthDatabase,
  column: "id" | "route_key",
  value: string,
): Promise<PublicShareRecord | null> {
  const row = await firstRow<PublicShareRow>(
    db,
    `SELECT id, route_key, pathname, search, created_at, updated_at, last_accessed_at, created_by_user_id, created_by_email
      FROM public_shares
      WHERE ${column} = ?`,
    [value],
  );

  return row ? mapPublicShare(row) : null;
}

function mapPublicShare(row: PublicShareRow): PublicShareRecord {
  return {
    createdAt: row.created_at,
    createdByEmail: row.created_by_email,
    createdByUserId: row.created_by_user_id,
    id: row.id,
    lastAccessedAt: row.last_accessed_at,
    pathname: row.pathname,
    routeKey: row.route_key,
    search: row.search,
    updatedAt: row.updated_at,
  };
}

function isPublicShareReaderRequestAllowed(access: PublicShareAccess, url: URL): boolean {
  if (!url.pathname.startsWith("/read/")) {
    return false;
  }

  const sharedLibraryId = pathSegments(access.share.pathname)[1];
  return (
    Boolean(sharedLibraryId) &&
    url.searchParams.get("libraryId") === sharedLibraryId &&
    Boolean(url.searchParams.get("chapterId"))
  );
}

function isPublicShareDownloadedChapterApiRequestAllowed(seriesId: string, url: URL): boolean {
  if (url.searchParams.get("libraryId") !== seriesId) {
    return false;
  }

  return /^\/api\/app\/library\/chapters\/[^/]+\/pages(?:\/[^/]+)?$/.test(url.pathname);
}

function pathSegments(pathname: string): string[] {
  return pathname.split("/").filter(Boolean).map(encodeURIComponentSafe);
}

function encodeURIComponentSafe(value: string): string {
  try {
    return encodeURIComponent(decodeURIComponent(value));
  } catch {
    return encodeURIComponent(value);
  }
}

function mergeCookieHeader(
  existingCookieHeader: string | null | undefined,
  publicShareCookie: string,
): string {
  const existingCookies = existingCookieHeader
    ?.split(";")
    .map((cookie) => cookie.trim())
    .filter((cookie) => cookie && !cookie.startsWith(`${PUBLIC_SHARE_COOKIE_NAME}=`));

  return [...(existingCookies ?? []), publicShareCookie].join("; ");
}

function isD1Database(db: AuthDatabase): db is D1Database {
  return typeof (db as { batch?: unknown }).batch === "function";
}

async function firstRow<T>(
  db: AuthDatabase,
  sql: string,
  params: Array<string | null>,
): Promise<T | null> {
  if (isD1Database(db)) {
    return (
      (await db
        .prepare(sql)
        .bind(...params)
        .first<T>()) ?? null
    );
  }

  return ((db as LocalAuthDatabase).prepare(sql).get(...params) as T | undefined) ?? null;
}

async function runStatement(
  db: AuthDatabase,
  sql: string,
  params: Array<string | null>,
): Promise<{ changes: number }> {
  if (isD1Database(db)) {
    const statement = db.prepare(sql);
    const result = await (params.length > 0 ? statement.bind(...params) : statement).run();
    return { changes: result.meta.changes ?? 0 };
  }

  const result = (db as LocalAuthDatabase).prepare(sql).run(...params);
  return { changes: result.changes };
}
