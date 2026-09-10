import { defineConfig } from "orval";

const openApiTarget = process.env.ORVAL_OPENAPI_TARGET ?? "./openapi.json";

export default defineConfig({
  mangaServer: {
    input: {
      target: openApiTarget,
    },
    output: {
      formatter: "oxfmt",
      mock: true,
      baseUrl: {
        baseUrl: "",
        getBaseUrlFromSpecification: false,
      },
      clean: true,
      client: "svelte-query",
      mode: "tags-split",
      schemas: "./src/generated/model",
      target: "./src/generated/endpoints",
      override: {
        fetch: {
          includeHttpResponseReturnType: false,
        },
        mutator: {
          name: "customFetch",
          path: "./src/orval-mutator.ts",
        },
        query: {
          useInfinite: true,
          shouldSplitQueryKey: true,
        },
      },
    },
  },
});
