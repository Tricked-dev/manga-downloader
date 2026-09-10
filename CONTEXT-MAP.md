# Context map

- [Product language](CONTEXT.md): Chapter Pages, Local Library Chapters, Download Work State, archive lifecycle, and activity facts shared by the server and clients.
- [Sources](crates/sources/CONTEXT.md): native source adapters, the static catalog, browser capture, and media requests.
- [Chapter storage](crates/storage/CONTEXT.md): BBF containers, original and upscaled variants, metadata, and safe mapped reads.
- [Upscaling](crates/upscale/CONTEXT.md): model selection, the bounded inference worker, and lossless AVIF output.
- [Persistence](crates/persistence/CONTEXT.md): SQLite and PostgreSQL schemas, connection selection and write coordination.
- [Authentication](crates/server/src/api/auth/CONTEXT.md): OIDC, durable sessions, bearer credentials and public sharing.
- [Web application](web/CONTEXT.md): static assets, browser state and API integration.

Client extensions use the shared product language. Their context entry is added as that boundary is migrated.
