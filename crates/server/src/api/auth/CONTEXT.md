# Authentication

An **OIDC Login Attempt** binds a PKCE verifier and nonce to one browser and one configuration.
It is short-lived, single-use and kept in memory; restarting interrupts pending sign-ins.

An **Auth Session** is an opaque browser credential whose hash and verified user identity are
stored in the application's database. Its configuration fingerprint invalidates sessions when
credentials or issuer settings change. Bearer API keys remain independent client credentials.

A **Public Share** grants read access to one library series and the downloaded chapters actually
owned by that series. The sharing cookie references a revocable database record; query parameters
never establish chapter ownership. API responses require origin authorization even when a browser
or intermediary has cached the content.

See [configuration and protocol details](../../../../../docs/authentication.md).
