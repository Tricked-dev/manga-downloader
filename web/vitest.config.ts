import { playwright } from "@vitest/browser-playwright";
import tailwindcss from "@tailwindcss/vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, configDefaults } from "vitest/config";

const browserLaunchChannel = process.env.VITEST_BROWSER_CHANNEL ?? "chrome";
const browserExecutable = process.env.CHROMIUM_EXECUTABLE_PATH;
const configDir = dirname(fileURLToPath(import.meta.url));

function readCargoVersion(contents: string): string {
  const match = /^\s*version\s*=\s*"([^"]+)"/m.exec(contents);
  return match?.[1] ?? "unknown";
}

function readServerCargoVersion(): string {
  const candidates = [
    resolve(configDir, "../Cargo.toml"),
    resolve(process.cwd(), "../Cargo.toml"),
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
  plugins: [tailwindcss(), sveltekit()],
  test: {
    passWithNoTests: true,
    projects: [
      {
        extends: true,
        test: {
          exclude: [...configDefaults.exclude, "src/**/*.browser.test.ts"],
          name: "unit",
          include: ["src/**/*.test.ts"],
          setupFiles: ["src/test/setup.ts"],
        },
      },
      {
        extends: true,
        test: {
          include: ["src/**/*.browser.test.ts"],
          name: "browser",
          setupFiles: ["vitest-browser-svelte"],
          browser: {
            enabled: true,
            headless: true,
            provider: playwright({
              launchOptions: {
                ...(browserExecutable ? { executablePath: browserExecutable } : { channel: browserLaunchChannel }),
              },
            }),
            instances: [{ browser: "chromium" }],
          },
        },
      },
    ],
  },
});
