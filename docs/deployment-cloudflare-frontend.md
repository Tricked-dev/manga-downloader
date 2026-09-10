# Deploy the Frontend on Cloudflare

The web app is a SvelteKit app built with `@sveltejs/adapter-cloudflare`. The Cloudflare output directory is `apps/svelte/web/.svelte-kit/cloudflare`.

## Cloudflare Pages

Use these settings when connecting the GitHub repository to Cloudflare Pages:

| Setting | Value |
| --- | --- |
| Root directory | repository root |
| Build command | `pnpm run build:cloudflare` |
| Build output directory | `apps/svelte/web/.svelte-kit/cloudflare` |
| Node.js version | read from `.node-version` (`26.1.0`) |

Set these environment variables in the Pages project:

| Variable | Required | Notes |
| --- | --- | --- |
| `BACKEND_URL` | Yes | Backend API origin used by the frontend server/proxy. |
| `PUBLIC_API_BASE` | No | Browser-visible backend API origin. Leave unset to keep browser calls on the frontend's `/api/app` proxy. |
| `BACKEND_INTERNAL_URL` | No | Legacy server-side backend API origin override. Prefer `BACKEND_URL` for new deployments. |
| `BACKEND_API_KEY` | If backend auth is enabled | Add as a secret, not a plain text variable. |
| `BETTER_AUTH_SECRET` | If frontend auth is enabled | Add as a secret. |
| `BETTER_AUTH_URL` | If frontend auth is enabled | Public frontend origin, for example `https://manga.example.com`. |
| `OIDC_CLIENT_ID` | If frontend auth is enabled | OIDC client ID. |
| `OIDC_CLIENT_SECRET` | If frontend auth is enabled | Add as a secret. |
| `OIDC_ISSUER_URL` | If frontend auth is enabled | OIDC discovery issuer URL. |

If frontend auth is enabled on Cloudflare, bind a D1 database as `AUTH_DB`. Without that binding, auth storage falls back to local SQLite, which is only appropriate for local Node runs.

## Wrangler

For direct Worker deploys, run from the repository root:

```bash
pnpm --dir frontend run deploy:cloudflare:dry-run
pnpm --dir frontend run deploy:cloudflare
```

These Worker deploy scripts collect the current Git branch, commit, commit date,
commit subject, upstream ref, and repository URL, then pass them to Wrangler with
`--var` so the frontend deployment metadata card is populated at runtime.

For local Cloudflare runtime preview:

```bash
pnpm --dir frontend run preview:cloudflare
```

The Worker config lives at `apps/svelte/web/wrangler.toml`. Set Worker secrets with `wrangler secret put` from `apps/svelte/web` before deploying any secret-backed environment.
