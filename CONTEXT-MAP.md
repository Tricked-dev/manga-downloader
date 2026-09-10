# Context map

- [Product language](CONTEXT.md): Chapter Pages, Local Library Chapters, Download Work State, archive lifecycle, and activity facts shared by the server and clients.
- [Sources](crates/sources/CONTEXT.md): native source adapters, the static catalog, browser capture, and media requests.
- [Chapter storage](crates/storage/CONTEXT.md): BBF containers, original and upscaled variants, metadata, and safe mapped reads.
- [Upscaling](crates/upscale/CONTEXT.md): model selection, the GPU worker, and lossless AVIF output.

Persistence, the web application, and client extensions use the shared product language. Their context entries are added as those boundaries are migrated.
