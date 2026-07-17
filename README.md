# AgentBrain

**A compiler-free, project-level knowledge engine for coding agents.**

AgentBrain indexes a codebase and its hand-written knowledge documents into a
single SQLite database, then answers a query with a **self-contained Evidence
Packet** — the relevant knowledge, its ancestry, the code symbols it cites
(resolved to real `file:line` with inlined source), a self-assessment of whether
it can answer, and a recommended next action. It is built for one purpose: to let
a coding agent get a *trustworthy, cited* answer in **one round-trip** instead of
grepping the repo across many.

> Reference project: verified against Unreal Engine's **Lyra** sample
> (~725 files → 3,190 symbols, 14,607 edges, 74 knowledge units).

---

## Why

Plain RAG over source code fails: embeddings blur symbol identity, chunk
boundaries cut through functions, and a top-k of "similar-looking" lines gives an
agent no way to know whether the answer is *correct* or *current*. AgentBrain
takes a different stance:

- **Knowledge is authored, code is mechanical.** Symbols, includes and call edges
  are extracted from source; *why it is designed this way*, responsibilities,
  boundaries and end-to-end flows live in hand-written Markdown that is compiled
  into structured Knowledge Units.
- **Every answer is grounded and self-assessing.** Each document claim is bound to
  a real code location; the engine verifies the binding, flags drift, and grades
  its own answerability.
- **One binary, no toolchain.** The scanner is purely lexical — no compiler, no
  language server, no build system.

---

## The compiler-free red line (design constraint #0)

The default scanner is **read-only and compiler-independent**. It never invokes:

- Clang / clangd / `compile_commands.json`
- Unreal build tools, `.uproject`/`.Build.cs`, or the Unreal Editor
- any project script

It uses lexical regex scanning for symbols and explicit `#include` / `import`
edges, plus a brace-scope state machine for call edges. This trades complete C++
semantic resolution for **predictable cost, zero deployment friction, and safe,
side-effect-free scanning**. Its accuracy boundaries are documented honestly in
[`ARCHITECTURE.md`](ARCHITECTURE.md) — nothing is hidden behind a false-precise
result.

---

## Install & build

Requires a recent stable **Rust** toolchain (edition 2024, Rust ≥ 1.85). SQLite
is bundled via `rusqlite` — no system SQLite needed.

```bash
git clone https://github.com/luser-dami/AgentBrain.git
cd AgentBrain
cargo build --release
# binary: ./target/release/brain-rs
```

---

## Quick start

**One brain per project, one entry per project root.** Everything a project
owns converges under a single `.brain/` home:

```
<project>/.brain/
  ├─ brain.toml      project config          (tracked)
  ├─ knowledge/      project-private docs    (tracked)
  ├─ packs/          project-level packs     (optional)
  └─ index/brain.db  generated index         (gitignored)
```

Config is discovered project-first (`.brain/brain.toml`, then a legacy
root-level `brain.toml`, then the engine default), and symbols, graph and
knowledge never mix across projects.

```bash
# 0) scaffold the project brain home (.brain/brain.toml + .brain/knowledge/)
brain-rs --project-root /path/to/project init

# 1) scan source -> symbols / edges / files   (incremental)
brain-rs --project-root /path/to/project scan

# 2) compile project knowledge docs (from <project>/knowledge/)
brain-rs --project-root /path/to/project compile

# 3) query -> self-contained Evidence Packets (assembled by default)
brain-rs --project-root /path/to/project query "how does weapon deal damage"
```

**Shared knowledge packs.** Reusable, ecosystem-scoped knowledge bases (e.g.
`ue-lyra`) live as directories under `packs/` — engine-level (`<engine>/packs/`)
or project-level (`<project>/.brain/packs/`, which wins). Each pack is **one
knowledge base = one database** (`<pack>/.brain/pack.db`), built once and bound
late:

```bash
# build a shared pack's own index (docs live directly in the pack dir)
brain-rs compile --pack packs/ue-lyra
```

A project opts into packs via `enabled_packs = ["ue-lyra"]` in its
`brain.toml` — a UE project enables UE packs and never sees frontend
knowledge. At query time every enabled brain is searched and the results are
fused, with each hit labelled by its brain. Pack symbol bindings resolve
**late**, against the querying project's code index, so the same pack reports
`verified` claims in one project and honest `unresolved` drift in another.

`scan` and `compile` are decoupled: editing a document only needs `compile`;
`scan` is incremental (unchanged files are skipped via mtime + BLAKE3 hash).

---

## Commands

| Command | What it does |
|---------|--------------|
| `init` | Scaffold the shared knowledge-base template into `.brain/` (idempotent); `init --pack <dir>` scaffolds a pack. Projects and packs share one template source, so organisation stays aligned. |
| `scan` | Parallel, incremental lexical scan of source → `symbols` / `edges` / `files`. |
| `compile` | Split knowledge docs into Knowledge Units; extract claims (graded `extracted`/`inferred`, verified against code), evidence, symbol cross-refs; run the Chunk Contract gate. `compile --pack <dir>` builds a shared pack's own db instead. |
| `query <text>` | **Multi-route retrieval fusion across every enabled brain** (BM25 + exact symbol + code graph, blended by Reciprocal Rank Fusion). Assembles top-3 self-contained Evidence Packets **by default**. |
| `locate <symbol>` | Find a code symbol's definition site (project brain only). |
| `refs <symbol>` | Reverse lookup across all brains: which Knowledge Units reference this symbol (with doc/code drift warnings). |
| `graph <kind> <symbol>` | Code-graph query. `kind` ∈ `callers` / `callees` (symbol-level call edges, multi-hop) · `deps` / `dependents` (file-level includes) · `impact`. |
| `status` | Index statistics (per-table counts, gate grades, timestamps). |
| `contract` | **Chunk Contract audit**: pass rate + every degraded/quarantined unit with the named rule it failed and why. |
| `lint` | **Hard pre-compile gate** for knowledge-base hygiene: document format, directory layout, and `enabled_packs` legality; named rules, `--json`, exits non-zero on errors. `lint --pack <dir>` lints one pack. |

### Common flags

| Flag | Applies to | Meaning |
|------|-----------|---------|
| `--project-root <path>` | all | Root of the project to index (default: `.`). |
| `--config <path>` | all | Path to a `brain.toml` (default: `<project>/.brain/brain.toml`, else `<project>/brain.toml`, else the engine's bundled one). |
| `--state-dir <path>` | all | Where to write the index (overrides `[index].state_dir`). |
| `--json` | most | Machine-readable JSON output (for agent/MCP consumption). |
| `--brief` | `query` | Return a lightweight ranked list instead of full Evidence Packets. |
| `--scope <tier>` | `query` | Granularity filter: `overview` (project/domain) · `unit` (module/feature/file) · `section` · `detail` · `all`. |
| `--depth <n>` | `graph` | Max graph traversal depth. |

Example — query at a chosen granularity, as JSON:

```bash
brain-rs --project-root /path/to/project query "elimination scoring streak bonus" --scope unit --json
```

---

## Configuration (`brain.toml`)

Configuration is a single TOML file. **Every field is optional** — omit the file
entirely and sensible defaults apply. Select a custom file with `--config`. A
`brain.local.toml` is git-ignored for local overrides.

```toml
# ─────────────────────────────────────────────────────────────
[scan]
# Allowlist of scan roots, relative to --project-root.
# Empty (or omitted) = scan the whole project root.
include_dirs = ["Source", "Plugins"]

# Excludes WIN over include_dirs. Match by name, relative path,
# or a simple "*" wildcard segment.
exclude_patterns = [
  ".git", ".brain", "target",
  "Binaries", "Build", "DerivedDataCache", "Intermediate", "Saved",
  "node_modules", "dist", "obj",
  "Plugins/*/ThirdParty",
]

# File extensions to scan (leading dot optional, case-insensitive).
include_extensions = [
  ".cpp", ".c", ".h", ".hpp", ".cc", ".cxx", ".hh", ".hxx",
  ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".py", ".cs",
]

# Skip files larger than this (KiB). Guards against generated blobs.
max_file_size_kib = 1024

# ─────────────────────────────────────────────────────────────
[index]
# Where the index/state lives. Relative paths resolve against the
# binary's package dir unless absolute. Overridden by --state-dir.
state_dir = ".brain"

# Project-private knowledge doc roots, relative to --project-root.
# Scanned RECURSIVELY; documents live directly under these roots.
docs_dirs = [".brain/knowledge"]

# Shared knowledge packs enabled at query time (one pack = one db).
# Resolved: <project>/packs/<name> first, then <engine>/packs/<name>.
enabled_packs = []

# Optional Context-Envelope identity. Falls back to the project
# directory name when unset.
# repo = "MyProject"

# Optional default domain/system identity, applied to a document
# only when its own frontmatter declares none.
# system = "Combat"

# ─────────────────────────────────────────────────────────────
[retrieval]
# Max results returned by `query`.
max_results = 10

# Max hops for `graph` traversal (also the cap for --depth).
max_graph_depth = 3

# Safety cap on nodes visited during a graph query.
max_graph_nodes = 2000
```

### Configuration reference

| Section | Key | Type | Default | Purpose |
|---------|-----|------|---------|---------|
| `[scan]` | `include_dirs` | string[] | `[]` (whole root) | Allowlist of scan roots. |
| `[scan]` | `exclude_patterns` | string[] | `.git`, `.brain`, `target`, `Binaries`, `Build`, `DerivedDataCache`, `Intermediate`, `Saved`, `node_modules`, `dist`, `obj` | Excluded names/paths/`*` globs. Wins over includes. |
| `[scan]` | `include_extensions` | string[] | `ts tsx js jsx mjs cjs py cpp c h hpp cc cxx hh hxx` | Extensions to scan (dot optional). |
| `[scan]` | `max_file_size_kib` | int | `1024` | Skip files larger than this. |
| `[index]` | `state_dir` | string | `.brain` | Index/state location. Overridden by `--state-dir`. |
| `[index]` | `docs_dirs` | string[] | `[".brain/knowledge", "knowledge"]` | Project knowledge doc roots (recursive), relative to project root. |
| `[index]` | `enabled_packs` | string[] | `[]` | Shared knowledge packs to query, resolved `.brain/packs/` → `packs/` → engine `packs/`. |
| `[index]` | `repo` | string? | project dir name | Context-Envelope repo identity. |
| `[index]` | `system` | string? | none | Default domain identity when a doc's frontmatter omits one. |
| `[retrieval]` | `max_results` | int | `10` | Results per `query`. |
| `[retrieval]` | `max_graph_depth` | int | `3` | Max `graph` hops / `--depth` cap. |
| `[retrieval]` | `max_graph_nodes` | int | `2000` | Node-visit safety cap for `graph`. |

---

## Knowledge documents

Code gives *mechanical* facts; **hand-written Markdown gives meaning** — why a
thing is designed this way, its responsibilities, boundaries, and end-to-end
flows. These docs are the engine's fuel and its single source of truth.

Documents are organized on a **scope-of-concern ladder** (largest → smallest),
declared by a YAML frontmatter field:

| Tier | frontmatter | Scope | `--scope` |
|------|-------------|-------|-----------|
| Architecture | `architecture:` | the whole project | `overview` |
| Domain | `domain:` | a cross-module functional area | `overview` |
| Module | `module:` | one code unit / folder | `unit` |
| Feature | `feature:` + `module:` | one atomic thing (an ability, an algorithm) | `unit` |
| (Detail) | inline `###` | inside a document | `detail` |

Each document is split into self-contained **Knowledge Units** along its heading
tree, then passed through the **Chunk Contract** admission gate
(accepted / degraded / quarantined). Claims, boundaries and `` `symbol` defined at
`path:line` `` evidence bindings are extracted and cross-linked to code.

**Authoring is a spec, not a guess.** See
[`AUTHORING.md`](AUTHORING.md) for: where files go, the
document skeleton, the heading-keyword → semantic-kind table, and the
`compile → contract / refs / query --assemble` maintenance loop. Every rule there
is derived from the engine's actual parsing behavior with code references.

---

## How retrieval works

`query` fuses three independent recall routes with **Reciprocal Rank Fusion**, so
ranking stays explainable (each hit is tagged with the routes that surfaced it):

| Route | Signal | Recalls |
|-------|--------|---------|
| **bm25** | FTS5 full-text | natural-language relevance |
| **symbol** | exact code symbols in the query → reverse-lookup | precise, high-confidence |
| **graph** | 1-hop code-graph neighbors of the query's symbols | associative ("things around what you asked") |

Each top hit is then assembled into a self-contained **Evidence Packet**:
ancestor context, full body, child units, claims/boundaries, layered evidence
(primary author-cited / supporting mentions / graph relations) **with inlined
source excerpts**, plus an `answerability` verdict
(`sufficient` / `partial` / `insufficient`) and a `recommended_action`
(`proceed_with_evidence` / `proceed_with_caveats` / `fallback_to_source`). The
goal: enough in one packet that the agent rarely needs a second round-trip.

---

## Project layout

```text
AgentBrain/
├─ Cargo.toml
├─ brain.toml              # scanner / index / retrieval configuration
├─ ARCHITECTURE.md         # deep design doc (data flow, schema, pipelines, limits)
├─ TODO.md                 # tracked technical debt (e.g. decl/def resolution plan)
├─ knowledge/
│  ├─ AUTHORING.md         # knowledge-document authoring & maintenance spec
│  └─ docs/                # indexed Markdown knowledge (domains/ modules/ features/)
├─ src/
│  ├─ main.rs              # command dispatch
│  ├─ cli.rs               # CLI (clap) definitions
│  ├─ config.rs            # brain.toml model + defaults
│  ├─ model.rs             # core data structures
│  ├─ storage.rs           # paths + SQLite schema + sharded scan DBs
│  ├─ graph.rs             # code-graph queries
│  ├─ index/               # knowledge layer (split by responsibility)
│  │  ├─ mod.rs            #   compile orchestration + shared helpers
│  │  ├─ chunk.rs          #   heading-tree splitting + frontmatter
│  │  ├─ extract.rs        #   symbol / evidence / claim extraction
│  │  ├─ contract.rs       #   Chunk Contract gate + audit
│  │  ├─ packet.rs         #   Evidence Packet assembly & rendering
│  │  └─ retrieve.rs       #   multi-route fusion + locate / refs / status
│  └─ scanner/             # code layer (per-language lexical scanners)
│     ├─ mod.rs            #   parallel sharded scan + merge
│     ├─ common.rs         #   shared helpers (brace-scope call edges)
│     ├─ cpp.rs / typescript.rs / python.rs
├─ .brain/                 # generated index (git-ignored)
└─ target/                 # build artifacts (git-ignored)
```

---

## SQLite index

Everything lands in one WAL-mode SQLite database (`.brain/index/brain.db`) with
FTS5 for BM25. Tables: `files` (incremental hashes), `symbols`, `edges`, `nodes`
(Knowledge Units), `nodes_fts`, `claims`, `node_refs` (doc↔code bindings),
`contract_violations`, `metadata`. See [`ARCHITECTURE.md`](ARCHITECTURE.md) for
the full schema and pipeline internals.

---

## Status & roadmap

Implemented: rich Knowledge-Unit structuring, an auditable Chunk Contract gate,
self-assessing Evidence Packets with inlined source, multi-route (BM25 + symbol +
graph) fusion, and layered-granularity retrieval.

Known limitations are stated honestly in `ARCHITECTURE.md` §7 (lexical scanning
is an approximation; no vector/semantic search yet; no MCP server yet). Tracked
next steps live in [`TODO.md`](TODO.md), including a clangd-inspired
declaration/definition resolution plan and an MCP integration.

---

## License

MIT.
