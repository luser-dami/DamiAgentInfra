# Knowledge documents

Place standalone Markdown knowledge documents here. These files are the auditable
source for document search; generated SQLite/FTS indexes are written under
`.brain/` and ignored by Git.

This directory intentionally does not read or depend on the legacy `.pi` Brain
knowledge files.

## How to write and maintain these documents

**Read [`../AUTHORING.md`](../AUTHORING.md) before adding or editing a document.**
It is the maintenance spec, derived from the engine's actual parsing behaviour,
and covers:

- **Where files go** — the four-tier scope ladder (architecture → domain →
  module → feature) plus inline detail, and the `domains/` / `modules/` /
  `features/` layout.
- **How to write** — the standard document skeleton and which `##` headings
  trigger which semantic `kind`.
- **How to change** — recompile + the three self-check commands
  (`contract` / `refs` / `query --assemble`) that form the quality feedback loop.

The spec lives one level up (in `knowledge/`, outside `docs/`) on purpose: only
`docs/` is indexed, so the guide itself never pollutes the search index.

## The scope ladder (largest → smallest scope of concern)

| Tier | frontmatter | Scope of concern | `--scope` |
|------|-------------|------------------|-----------|
| Architecture | `architecture:` | the whole project | `overview` |
| Domain | `domain:` | a cross-module functional area | `overview` |
| Module | `module:` | one code unit / folder | `unit` |
| Feature | `feature:` + `module:` | one atomic thing (ability, algorithm) | `unit` |
| (Detail) | inline `###` | inside a document | `detail` |

> `system:` is still accepted as a backward-compatible alias for `domain:`.

## Layout

```
knowledge/
  AUTHORING.md      # the spec (NOT indexed)
  docs/             # indexed knowledge documents
    README.md       # this file
    Architecture.md # whole-project entry view (frontmatter: architecture:)
    domains/        # cross-module functional areas (frontmatter: domain:)
    modules/        # single code units (frontmatter: module:)
    features/       # atomic things — abilities, algorithms (feature: + module:)
```
