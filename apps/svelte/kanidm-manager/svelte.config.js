import adapter from "@sveltejs/adapter-cloudflare";
import { relative, sep } from "node:path";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  compilerOptions: {
    runes: ({ filename }) => {
      const relativePath = relative(import.meta.dirname, filename);
      return relativePath.toLowerCase().split(sep).includes("node_modules") ? undefined : true;
    },
  },
  kit: {
    adapter: adapter(),
    alias: {
      $components: "src/components",
      "$components/*": "src/components/*",
    },
  },
};

export default config;
