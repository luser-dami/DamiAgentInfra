# DamiAgentInfra

A toolbox of **independent modules** for AI coding agents. Agents are only as
good as the tools around them — this repository collects those tools. It is
deliberately not a monolith: every module is self-contained, installable, and
versioned on its own. Copy any single module directory out and it builds,
tests, and publishes alone.

## Modules

| Module   | Language   | Status        | Install one-liner                        |
| -------- | ---------- | ------------- | ---------------------------------------- |
| brain/   | Rust       | active        | `cd brain && cargo build --release`      |
| docs/    | TypeScript | in extraction | `cd docs && npm install && npm run build`|
| harness/ | TypeScript | in extraction | `cd harness && npm install`              |
| dami/    | Rust       | planned       | —                                        |

- **brain/** — a compiler-free project knowledge index & retrieval engine for
  coding agents: lexical + tree-sitter AST scanning into SQLite FTS5 plus a
  symbol graph, Markdown knowledge compiled into Knowledge Units, queries
  answered with self-contained Evidence Packets. See
  [brain/README.md](brain/README.md).
- **docs/** — a standalone external-document store merging codebase-wiki
  generation and experience/learnings record+recall over one store and one
  full-text search index. CLI: `dami-docs wiki gen|lint`,
  `dami-docs recall put|list`, `dami-docs search <query>`.
- **harness/** — a local harness resource manager: manage
  skills/rules/agents/hooks/MCP-config as files and install/reconcile them
  into agent tool directories (`.claude/`, `.codex/`, `.omp/`, …). Personal
  and standalone: no git remotes, no teams.
- **dami/** — an optional tiny Rust aggregator CLI that discovers `dami-*`
  executables on PATH (git-style) and forwards argv. It knows nothing about
  specific tools; the toolbox works fully without it.

## Design rules

1. **Self-contained directories.** Each module has its own manifest, lockfile,
   tests, README, and release lifecycle. There is no root build config.
2. **Zero inter-module code dependencies.** Modules may not import from each
   other. Shared behavior is a written convention, not shared code.
3. **Contract-by-convention.** The common CLI/output contract lives in
   [docs/tool-contract.md](docs/tool-contract.md); each module implements it
   independently (~50 lines of boilerplate).
4. **Per-module versioning.** Releases are tagged `<module>-vX.Y.Z` (e.g.
   `brain-v0.3.0`). There is no repo-wide version.
5. **Skills ship inside each module.** Whatever an agent needs to operate a
   tool (skill files, prompts, examples) travels with that module's directory.

## The tool contract

Every tool in this toolbox exposes `<tool> <verb> [args] [--json]`: JSON on
stdout with `--json`, human output on stderr, stable exit codes, and a
`--describe` self-description that feeds future MCP servers and skill
generators. Full spec: [docs/tool-contract.md](docs/tool-contract.md).

## License

MIT — see [LICENSE](LICENSE). Portions of `docs/` and `harness/` are derived
from Tencent's teamai-cli (MIT); third-party attribution is in
[NOTICE](NOTICE).
