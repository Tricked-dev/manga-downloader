import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import { relative, sep } from "node:path";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess({ script: true }),
  compilerOptions: {
    // Dependency filenames include the temporary Nix build root. Scope CSS to
    // a project-relative path so identical sources produce identical assets.
    cssHash: ({ filename, css, hash }) =>
      `svelte-${hash(filename ? relative(import.meta.dirname, filename).split(sep).join("/") : css)}`,
    // Defaults to rune mode for the project, execept for `node_modules`. Can be removed in svelte 6.
    runes: ({ filename }) => {
      const relativePath = relative(import.meta.dirname, filename);
      const pathSegments = relativePath.toLowerCase().split(sep);
      const isExternalLibrary = pathSegments.includes("node_modules");

      return isExternalLibrary ? undefined : true;
    },
  },
  kit: {
    adapter: adapter({ fallback: "index.html" }),
    version: { name: process.env.MANGA_WEB_VERSION },
    alias: {
      $components: "src/components",
      "$components/*": "src/components/*",
    },
  },
};

export default config;
