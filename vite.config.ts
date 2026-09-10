import { defineConfig } from "vite-plus";

const generatedInputExcludes = [
  "!**/.svelte-kit/**",
  "!**/node_modules/.vite/**",
  "!**/node_modules/.vite-temp/**",
  "!**/*.tsbuildinfo",
];

export default defineConfig({
  fmt: {
    ignorePatterns: ["**/build/**", "**/.svelte-kit/**", "**/node_modules/**"],
  },
  lint: {
    categories: {
      correctness: "error",
      suspicious: "error",
    },
    env: {
      browser: true,
      node: true,
    },
    globals: {
      $bindable: "readonly",
      $derived: "readonly",
      $effect: "readonly",
      $host: "readonly",
      $inspect: "readonly",
      $page: "readonly",
      $props: "readonly",
      $state: "readonly",
    },
    ignorePatterns: ["**/build/**", "**/.svelte-kit/**", "**/node_modules/**"],
    plugins: ["oxc", "import"],
  },
  run: {
    cache: {
      scripts: true,
      tasks: true,
    },
    tasks: {
      "build:web": {
        command: "vp build",
        cwd: "apps/svelte/web",
        input: [{ auto: true }, ...generatedInputExcludes],
        output: ["apps/svelte/web/.svelte-kit/**"],
      },
      "bench:web": {
        command: "vitest bench --run",
        cwd: "apps/svelte/web",
        input: [{ auto: true }, ...generatedInputExcludes],
      },
      "check:all": {
        command: "vp check",
        input: [{ auto: true }, ...generatedInputExcludes],
      },
      "test:api-client": {
        command: "vitest run",
        cwd: "libs/svelte/api-client",
        input: [{ auto: true }, ...generatedInputExcludes],
      },
      "test:web": {
        command: "vitest run",
        cwd: "apps/svelte/web",
        input: [{ auto: true }, ...generatedInputExcludes],
      },
    },
  },
});
