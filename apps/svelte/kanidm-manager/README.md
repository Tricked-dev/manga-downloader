# Kanidm Manager

SvelteKit app for managing Kanidm OAuth2 clients, groups, users, and Manga Server Better Auth OIDC settings.

The configured Kanidm account must already have the server-side rights for the actions exposed here. The Users tab requires person administration privileges, for example membership in `idm_people_admins` or the equivalent people-management privilege group for your Kanidm version.

## Cloudflare Workers and Pages

This app uses `@sveltejs/adapter-cloudflare` and `wrangler.jsonc`.

Required non-secret vars:

- `KANIDM_BASE_URL`
- `MANGA_SERVER_BASE_URL`

Required secrets:

- `KANIDM_USERNAME`
- `KANIDM_PASSWORD`
- `MANAGER_ACCESS_TOKEN`

Optional secret:

- `MANGA_SERVER_API_KEY` when the Manga Server settings API requires bearer auth.

The app fails closed when backend configuration is present but
`MANAGER_ACCESS_TOKEN` is missing. Send the token as either
`Authorization: Bearer <token>` or HTTP Basic auth password.

For Cloudflare Pages, put the project behind Cloudflare Access when the UI
itself should be private. The app-level token protects all SvelteKit SSR and
API actions, while platform-served static assets can still be visible without
an outer Access policy.

Set Worker production secrets from this app directory:

```sh
pnpm exec wrangler secret put KANIDM_USERNAME
pnpm exec wrangler secret put KANIDM_PASSWORD
pnpm exec wrangler secret put MANAGER_ACCESS_TOKEN
pnpm exec wrangler secret put MANGA_SERVER_API_KEY
```

Set Pages production secrets from this app directory:

```sh
pnpm exec wrangler pages secret put KANIDM_USERNAME --project-name manga-server-kanidm-manager
pnpm exec wrangler pages secret put KANIDM_PASSWORD --project-name manga-server-kanidm-manager
pnpm exec wrangler pages secret put MANAGER_ACCESS_TOKEN --project-name manga-server-kanidm-manager
pnpm exec wrangler pages secret put MANGA_SERVER_API_KEY --project-name manga-server-kanidm-manager
```

Local Worker preview can use `.dev.vars`; start from `.dev.vars.example`.

Useful commands:

```sh
pnpm run types:cloudflare
pnpm run preview:cloudflare
pnpm run preview:pages
pnpm run deploy:cloudflare:dry-run
pnpm run deploy:cloudflare
pnpm run deploy:pages
```
