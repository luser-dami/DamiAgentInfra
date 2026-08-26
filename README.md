# DamiAgentInfra

A toolbox of **independent modules** for AI coding agents. Agents are only as
good as the tools around them — this repository collects those tools. It is
deliberately not a monolith: every module is self-contained, installable, and
versioned on its own. Copy any single module directory out and it builds,
tests, and publishes alone.

## Modules

| Module   | Language   | Status        | Install one-liner                        |
| -------- | ---------- | ------------- | ---------------------------------------- |
| alexandria/ | Rust       | active        | `cd alexandria && cargo build --release` |
| harness/ | TypeScript | active | `cd harness && npm install`              |

- **alexandria/** — a compiler-free project knowledge index & retrieval engine for
  coding agents: lexical + tree-sitter AST scanning into SQLite FTS5 plus a
  symbol graph, Markdown knowledge compiled into Knowledge Units, queries
  answered with self-contained Evidence Packets. See
  [alexandria/README.md](alexandria/README.md).
  Its document layer is a strictly-architected knowledge format: a
  scope-of-concern tier ladder (architecture → domain → module → feature,
  plus error-sourced lessons), an explicit parsing contract (title keywords
  decide semantics, bullets decide claims, backticks decide code anchors),
  and three auditable quality gates (lint, Chunk Contract, tier schema) that
  keep the knowledge base from silently rotting. The format covers external
  documents too: per-project knowledge docs plus shared packs enabled on
  demand, late-bound against each querying project's code index.
- **harness/** — a local harness resource manager: manage
  skills/rules/agents/hooks/MCP-config as files and install/reconcile them
  into agent tool directories (`.claude/`, `.codex/`, `.omp/`, …). Personal
  and standalone: no git remotes, no teams.
## Design rules

1. **Self-contained directories.** Each module has its own manifest, lockfile,
   tests, README, and release lifecycle. There is no root build config.
2. **Zero inter-module code dependencies.** Modules may not import from each
   other. Shared behavior is a written convention, not shared code.
3. **Contract-by-convention.** The common CLI/output contract lives in
   the *Tool contract* section below; each module implements it
   independently (~50 lines of boilerplate).
4. **Per-module versioning.** Releases are tagged `<module>-vX.Y.Z` (e.g.
   `alexandria-v0.3.0`). There is no repo-wide version.
5. **Skills ship inside each module.** Whatever an agent needs to operate a
   tool (skill files, prompts, examples) travels with that module's directory.

## The tool contract

Every tool in this toolbox exposes `<tool> <verb> [args] [--json]`. This is a
**convention**, not a library: each module implements it independently (~50
lines of boilerplate); there is deliberately no shared contract package.

- **Output channels.** Machine-readable output is JSON on stdout when
  `--json` is passed — stdout carries data only. Human output (progress,
  warnings, summaries, pretty-printed results without `--json`) goes to
  stderr, so agents can pipe stdout into `jq` without filtering noise.
- **Exit codes.** `0` success · `2` usage/argument error · `3` environment
  error (missing dependency/runtime) · `4` domain error (e.g. project not
  indexed, store not initialized). `1` is reserved for unexpected internal
  failures.
- **Self-description.** Every tool implements `<tool> --describe`, printing
  JSON to stdout: `name`, `version`, one-line `summary`, `contract` (the
  contract version implemented, currently `1`), and `verbs` — each verb with
  its argument schema as JSON Schema. This output is the single source of
  truth for future MCP servers and skill generators; they consume it instead
  of parsing help text.

This is **contract version 1**. Breaking changes increment the version;
modules opt into newer versions individually, in line with the per-module
release model (`<module>-vX.Y.Z`).

## License

MIT — see [LICENSE](LICENSE). Portions of `harness/` are derived from
Tencent's teamai-cli (MIT); third-party attribution is in
[NOTICE](NOTICE).
