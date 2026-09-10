# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`CONTEXT-MAP.md`** at the repo root. It points at one `CONTEXT.md` per context. Read each one relevant to the topic.
- **`docs/adr/`** for system-wide decisions that touch the area you're about to work in.
- **Context-scoped `docs/adr/` directories** for decisions owned by a specific context.

If any of these files don't exist, **proceed silently**. Don't flag their absence; don't suggest creating them upfront. The producer skill (`/grill-with-docs`) creates them lazily when terms or decisions actually get resolved.

## File structure

This repo uses a multi-context layout:

```
/
|-- CONTEXT-MAP.md
|-- docs/adr/                          # system-wide decisions
|-- crates/
|   |-- CONTEXT.md
|   `-- docs/adr/                      # Rust backend-specific decisions
|-- web/
|   |-- CONTEXT.md
|   `-- docs/adr/                      # Svelte frontend-specific decisions
`-- clients/
    |-- CONTEXT.md
    `-- docs/adr/                      # client-specific decisions
```

The concrete contexts may change over time. Treat `CONTEXT-MAP.md` as the source of truth when it exists.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the term as defined in the relevant `CONTEXT.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal: either you're inventing language the project doesn't use, or there's a real gap to note for `/grill-with-docs`.

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0007 (event-sourced orders) -- but worth reopening because..._
