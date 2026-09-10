import tailwindcss from "@tailwindcss/vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import Sonda from "sonda/sveltekit";

const analyze = process.env.ANALYZE === "true";
const configDir = dirname(fileURLToPath(import.meta.url));

function readCargoVersion(contents: string): string {
  const match = /^\s*version\s*=\s*"([^"]+)"/m.exec(contents);
  return match?.[1] ?? "unknown";
}

function readServerCargoVersion(): string {
  const candidates = [
    resolve(configDir, "../../../third_party/rust/Cargo.toml"),
    resolve(process.cwd(), "../../../third_party/rust/Cargo.toml"),
  ];

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return readCargoVersion(readFileSync(candidate, "utf8"));
    }
  }

  return "unknown";
}

export default defineConfig({
  define: {
    APP_BUILD_AT: JSON.stringify(new Date().toISOString()),
    SERVER_CARGO_VERSION: JSON.stringify(readServerCargoVersion()),
  },
  resolve: {
    alias: {
      "@better-auth/core/social-providers": new URL(
        "./src/lib/server/auth/better-auth-social-providers.ts",
        import.meta.url,
      ).pathname,
    },
  },
  build: {
    sourcemap: analyze,
  },
  plugins: [
    tailwindcss(),
    sveltekit(),
    Sonda({
      enabled: analyze,
      open: analyze,
      server: false,
      gzip: true,
    }),
  ],
});
