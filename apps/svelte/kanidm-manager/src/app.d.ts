declare global {
  namespace App {
    interface KanidmManagerEnv extends import("./worker-configuration").Env {
      KANIDM_USERNAME?: string;
      KANIDM_PASSWORD?: string;
      MANAGER_ACCESS_TOKEN?: string;
      MANGA_SERVER_API_KEY?: string;
      BACKEND_API_KEY?: string;
      BACKEND_INTERNAL_URL?: string;
      PUBLIC_API_BASE?: string;
      [key: string]: Fetcher | string | undefined;
    }

    interface Platform {
      env?: KanidmManagerEnv;
    }
  }
}

export {};
