import { env } from "$env/dynamic/private";
import type { D1Database } from "@cloudflare/workers-types";

const defaultAuthDbPath = "data/auth.db";

type LocalAuthDatabase = import("better-sqlite3").Database;
type AuthDatabase = D1Database | LocalAuthDatabase;

let localAuthDatabase: Promise<LocalAuthDatabase> | null = null;

export async function getAuthDatabase(platform?: App.Platform): Promise<AuthDatabase> {
  if (platform?.env.AUTH_DB) {
    return platform.env.AUTH_DB;
  }

  localAuthDatabase ??= createLocalAuthDatabase();
  return localAuthDatabase;
}

async function createLocalAuthDatabase(): Promise<LocalAuthDatabase> {
  const [{ mkdirSync }, path, { default: Database }] = await Promise.all([
    import("node:fs"),
    import("node:path"),
    import(/* @vite-ignore */ "better-sqlite3"),
  ]);

  const authDbPath = path.resolve(process.cwd(), env.AUTH_DB_PATH?.trim() ?? defaultAuthDbPath);
  mkdirSync(path.dirname(authDbPath), { recursive: true });

  const database = new Database(authDbPath);
  database.pragma("journal_mode = WAL");
  database.pragma("foreign_keys = ON");

  return database;
}
