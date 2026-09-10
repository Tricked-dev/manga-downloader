# Authentication

Rust handles browser sign-in and sessions. Set `AUTH_ENABLED=true`, `OIDC_ISSUER_URL`,
`OIDC_CLIENT_ID`, and, for confidential clients, `OIDC_CLIENT_SECRET`. These values can also
be set through the existing settings API. Configure `PUBLIC_URL=https://manga.example.com`
(or `--public-url`) and register exactly `https://manga.example.com/auth/callback` with the
issuer. URLs require HTTPS except for loopback development. The external URL is explicit;
forwarded headers cannot change the callback or cookie policy.

`GET /auth/login?next=/library` starts authorization code flow with PKCE. Rust verifies the
signed ID token, issuer, audience, expiry and nonce, and verifies the access-token hash when
present. Discovery and signing keys are cached for five minutes and refreshed once after
verification failure, allowing key rotation without exchanging a code twice. Login attempts
expire after ten minutes, are single-use, and are bound to an HttpOnly browser cookie.

Sessions last seven days. The database stores a hash of an opaque random cookie value,
with the user's verified identity and an authentication-configuration fingerprint. Sessions
survive restart but expire or become invalid when authentication settings change.
`POST /auth/logout` revokes the stored session and clears login and sharing cookies.
Browser writes require an `Origin` matching `PUBLIC_URL`; cookies are HttpOnly and SameSite=Lax,
and Secure on HTTPS. `GET /auth/session` returns configuration availability and current identity,
without tokens or client secrets. Settings responses mask existing secrets; sending the unchanged
mask preserves the existing value.

`BACKEND_API_KEY` remains a bearer credential for extensions, scripts and CLI clients. Bearer
requests do not require browser cookies or an Origin header. With authentication enabled but
incomplete OIDC configuration, the API stays protected and browser sign-in reports unavailable.
When both OIDC and the bearer key are disabled, the API is public, suitable for local use.

An authenticated owner can create or revoke sharing through
`POST` or `DELETE /auth/public-share/{series_id}`. Share links remain
`/library/{series_id}?public=1`; `/auth/public-share/{series_id}/open` establishes the scoped
sharing cookie. Sharing permits reading the selected series, its downloaded chapter pages,
and its saved cover. The server checks the chapter's stored series identity and does not trust
a `libraryId` query parameter. Sharing cannot modify library data or access other series.

The same session and sharing behavior is tested with SQLite and PostgreSQL. A local HTTP issuer
fixture covers PKCE exchange, cookie binding, replay, signing-key rotation and rejection of
incorrect issuer, audience, expiry, nonce, signature and access-token hash. Production issuer
setup still needs a registered OIDC client.
