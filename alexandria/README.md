# Alexandria

**A compiler-free, project-level knowledge engine for coding agents.**

Alexandria indexes a codebase and its hand-written knowledge documents into a
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
agent no way to know whether the answer is *correct* or *current*. Alexandria
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
git clone https://github.com/luser-dami/DamiAgentInfra.git
cd DamiAgentInfra/alexandria   # Alexandria lives in the alexandria/ subdirectory of this monorepo

# default build (offline, no neural dependencies)
cargo build --release

# build with the optional local neural embedder (Candle + MiniLM)
# see "Vector / semantic search" below
cargo build --release --features neural

# binary: ./target/release/alexandria
```
**Agent skill included**: [skills/alexandria/SKILL.md](skills/alexandria/SKILL.md)
teaches a coding agent when and how to use alexandria (query-first workflow,
lesson writing, feedback). Install it into your agent's skills directory —
e.g. via `dami-harness`, or copy it manually.

---

## Quick start

**One library per project, one entry per project root.** Everything a project
owns converges under a single `.alexandria/` home:

```
<project>/.alexandria/
  ├─ alexandria.toml      project config          (tracked)
  ├─ knowledge/      project-private docs    (tracked)
  ├─ packs/          project-level packs     (optional)
  └─ index/alexandria.db  generated index         (gitignored)
```

Config is discovered project-first (`.alexandria/alexandria.toml`, then a legacy
root-level `alexandria.toml`, then the engine default), and symbols, graph and
knowledge never mix across projects.

```bash
# 0) scaffold the project library home (.alexandria/alexandria.toml + .alexandria/knowledge/)
alexandria --project-root /path/to/project init

# 1) scan source -> symbols / edges / files   (incremental)
alexandria --project-root /path/to/project scan

# 2) compile project knowledge docs (from <project>/knowledge/)
alexandria --project-root /path/to/project compile

# 3) query -> self-contained Evidence Packets (assembled by default)
alexandria --project-root /path/to/project query "how does weapon deal damage"
```

**Shared knowledge packs.** Reusable, ecosystem-scoped knowledge bases (e.g.
`ue-engine`) live as directories under `packs/` at three levels, UE-plugin-style:
`<project>/.alexandria/packs/` (machine-local override) → `<project>/packs/`
(project plugins) → `<packs_root>/packs/` (engine plugins; `packs_root` is set
in `alexandria.toml`). Each pack is **one knowledge base = one database**
(`<pack>/.alexandria/pack.db`). Referencing a pack builds it: plain `compile`
builds the project library *and* every enabled pack:

```bash
# build a shared pack's own index (docs live directly in the pack dir)
alexandria compile --pack packs/ue-lyra
```

A project opts into packs via `enabled_packs = ["ue-lyra"]` in its
`alexandria.toml` — a UE project enables UE packs and never sees frontend
knowledge. At query time every enabled library is searched and the results are
fused, with each hit labelled by its library. Pack symbol bindings resolve
**late**, against the querying project's code index, so the same pack reports
`verified` claims in one project and honest `unresolved` drift in another.

`scan` and `compile` are decoupled: editing a document only needs `compile`;
`scan` is incremental (unchanged files are skipped via mtime + BLAKE3 hash).

---

## Commands

| Command | What it does |
|---------|--------------|
| `init` | Scaffold the shared knowledge-base template into `.alexandria/` (idempotent); `init --pack <dir>` scaffolds a pack. Projects and packs share one template source, so organisation stays aligned. |
| `scaffold <dir>` | **Generation-layer bridge**: derive a module document draft from the code index — real classes, dependencies, consumers and `file:line` evidence pre-filled; the agent writes only the semantics. Writes `.alexandria/knowledge/modules/<Name>.md`, never overwrites. |
| `scan` | Parallel, incremental lexical scan of source → `symbols` / `edges` / `files`. |
| `compile` | Split knowledge docs into Knowledge Units; extract claims (graded `extracted`/`inferred`, verified against code), evidence, symbol cross-refs; run the Chunk Contract gate. `compile --pack <dir>` builds a shared pack's own db instead. |
| `query <text>` | **Multi-route retrieval fusion across every enabled library** (BM25 + exact symbol + code graph + vector, blended by Reciprocal Rank Fusion; the vector lane ships an offline morphological embedder by default, with an optional local neural embedder). Assembles top-3 self-contained Evidence Packets **by default**. |
| `locate <symbol>` | Find a code symbol's definition site (project library only). |
| `refs <symbol>` | Reverse lookup across all libraries: which Knowledge Units reference this symbol (with doc/code drift warnings). |
| `graph <kind> <symbol>` | Code-graph query. `kind` ∈ `callers` / `callees` (symbol-level call edges, multi-hop) · `deps` / `dependents` (file-level includes) · `impact`. |
| `status` | Index statistics (per-table counts, gate grades, timestamps). |
| `contract` | **Chunk Contract audit**: pass rate + every degraded/quarantined unit with the named rule it failed and why. |
|| `feedback` | **Answer-feedback loop** (agent-driven): record a verdict (`useful`/`partial`/`wrong`/`stale`) with `--query/--node/--library/--note`; later packets on that unit carry the warning until fixed and `feedback --clear <node>` clears it. `--list` reviews. For lessons, the `applied-resolved`/`applied-failed` pair instead measures **Guard efficacy**: a Guard failing 2+ times in a row demotes the lesson in retrieval and adds a packet warning; one resolving 3+ times is flagged in `status` as a graduation candidate. Verdicts recorded against a section or the doc root are equivalent (lookup normalises to the document). |
| `lint` | **Hard pre-compile gate** for knowledge-base hygiene: document format, directory layout, and `enabled_packs` legality; named rules, `--json`, exits non-zero on errors. `lint --pack <dir>` lints one pack. |

### For agents: output formats

Three formats, three audiences: `text` (default, humans), `--format json`
(strict machine parsing), **`--format tagged` (recommended for LLM agents)** —
XML-ish semantic tags with explicit field boundaries and CDATA-wrapped
prose/source, so nothing needs un-escaping and code stays verbatim:

```bash
alexandria query "weapon spread heat" --format tagged
alexandria refs ULyraHealthSet --format tagged
alexandria locate OnEquipped --format tagged
alexandria graph callees OnEquipped --format tagged
```

Every packet carries its `node_id` + `library` — the exact address to attach
feedback to (see the feedback loop below).

### For agents: the feedback loop

Feedback is not a user chore — record it on the user's behalf. When the user
confirms, corrects or refutes an answer in natural language:

```bash
alexandria feedback stale --query "weapon spread heat"     --node "doc:features/WeaponSpreadHeat.md" --library ue-lyra     --action fell_back_to_source --note "curve names changed in latest code"
```

(`node`/`library` come from `query --json` hits.) From then on, every packet
for that unit warns about the recorded verdict — until the document is fixed
and `alexandria feedback --clear <node>` clears it. This is how the knowledge
base learns from real usage, per project.

### Common flags

| Flag | Applies to | Meaning |
|------|-----------|---------|
| `--project-root <path>` | all | Root of the project to index (default: `.`). |
| `--config <path>` | all | Path to a `alexandria.toml` (default: `<project>/.alexandria/alexandria.toml`, else `<project>/alexandria.toml`, else the engine's bundled one). |
| `--state-dir <path>` | all | Where to write the index (overrides `[index].state_dir`). |
| `--json` | most | Machine-readable JSON output (for agent/MCP consumption). |
| `--brief` | `query` | Return a lightweight ranked list instead of full Evidence Packets. |
| `--format <fmt>` | global | `text` (default) · `json` · `tagged` (XML-ish, tuned for LLM agents; supported on query/refs/locate/graph). Per-command `--json` ≡ `--format json`. |
|| `--scope <tier>` | `query` | Granularity filter: `overview` (project/domain) · `unit` (module/feature/file) · `section` · `detail` · `all`. |
|| `--context <slugs>` | `query` | Declare the task context (comma-separated) for exact lesson applicability matching: excludes demote ×0.5 (`excluded` + warning), applies-when hits boost ×1.25 (`match`), misses demote ×0.85 (`mismatch`). |
| `--depth <n>` | `graph` | Max graph traversal depth. |

Example — query at a chosen granularity, as JSON:

```bash
alexandria --project-root /path/to/project query "elimination scoring streak bonus" --scope unit --json
```

---

## Configuration (`alexandria.toml`)

Configuration is a single TOML file. **Every field is optional** — omit the file
entirely and sensible defaults apply. Select a custom file with `--config`. A
`library.local.toml` is git-ignored for local overrides.

```toml
# ─────────────────────────────────────────────────────────────
[scan]
# Allowlist of scan roots, relative to --project-root.
# Empty (or omitted) = scan the whole project root.
include_dirs = ["Source", "Plugins"]

# Excludes WIN over include_dirs. Match by name, relative path,
# or a simple "*" wildcard segment.
exclude_patterns = [
  ".git", ".alexandria", "target",
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
state_dir = ".alexandria"

# Project-private knowledge doc roots, relative to --project-root.
# Scanned RECURSIVELY; documents live directly under these roots.
docs_dirs = [".alexandria/knowledge"]

# Shared knowledge packs enabled at query time (one pack = one db).
# Referenced packs are built by `compile` and queried with the project.
# Resolved in order: <project>/.alexandria/packs/<name>, <project>/packs/<name>,
# then <packs_root>/packs/<name> (engine-level, UE's engine-plugins analog).
enabled_packs = []
# packs_root = "../DamiAgentInfra/alexandria"

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
| `[scan]` | `exclude_patterns` | string[] | `.git`, `.alexandria`, `target`, `Binaries`, `Build`, `DerivedDataCache`, `Intermediate`, `Saved`, `node_modules`, `dist`, `obj` | Excluded names/paths/`*` globs. Wins over includes. |
| `[scan]` | `include_extensions` | string[] | `ts tsx js jsx mjs cjs py cpp c h hpp cc cxx hh hxx` | Extensions to scan (dot optional). |
| `[scan]` | `max_file_size_kib` | int | `1024` | Skip files larger than this. |
| `[index]` | `state_dir` | string | `.alexandria` | Index/state location. Overridden by `--state-dir`. |
| `[index]` | `docs_dirs` | string[] | `[".alexandria/knowledge", "knowledge"]` | Project knowledge doc roots (recursive), relative to project root. |
| `[index]` | `enabled_packs` | string[] | `[]` | Shared knowledge packs to query, resolved `.alexandria/packs/` → `packs/` → engine `packs/`. |
| `[index]` | `repo` | string? | project dir name | Context-Envelope repo identity. |
| `[index]` | `system` | string? | none | Default domain identity when a doc's frontmatter omits one. |
| `[retrieval]` | `max_results` | int | `10` | Results per `query`. |
| `[retrieval]` | `max_graph_depth` | int | `3` | Max `graph` hops / `--depth` cap. |
| `[retrieval]` | `max_graph_nodes` | int | `2000` | Node-visit safety cap for `graph`. |
| `[vector]` | `enabled` | bool | `true` | Master switch for the vector lane and embedding refresh. |
| `[vector]` | `embedder` | string | `"hash-ngram"` | `"hash-ngram"` (built-in) or `"minilm-l6-v2"` (with the `neural` feature). |
| `[vector]` | `weight` | float | `0.8` | RRF fusion weight of vector hits (bm25 = 1.0, symbol = 2.0). |
| `[vector.neural]` | `model_dir` | string | `".alexandria/models/all-MiniLM-L6-v2"` | Directory containing `config.json`, `model.safetensors`, `tokenizer.json`. Local files only; no automatic download. |
| `[vector.neural]` | `max_tokens` | int | `256` | Embedding input budget in tokens; longer node text is truncated (overflowing nodes are reported at compile time). The main embedding-speed knob. |

---

## Knowledge documents

Code gives *mechanical* facts; **hand-written Markdown gives meaning** — why a
thing is designed this way, its responsibilities, boundaries, and end-to-end
flows. These docs are the engine's fuel and its single source of truth. They
are not RAG corpus: where files live, how they are written, and how the engine
parses them are all **explicit contracts**.

### Knowledge roots: one directory = one library

A **knowledge root** is a single directory with documents directly at the
root, organised by a four-tier template plus an experience tier:

```
<knowledge root>/
  Architecture.md     ← L0 architecture (`architecture:`) — the entry view
  domains/            ← L1 domains (`domain:`) — cross-module end-to-end flows
  modules/            ← L2 modules (`module:`) — single-code-unit responsibilities
  features/           ← L3 features (`feature:` + `module:`) — atomic key things
  lessons/            ← experience tier (`lesson:`) — resolved errors, off the ladder
```

There are exactly two kinds of knowledge base — the **project library**
(`.alexandria/knowledge/`; its index db also holds the code layer) and
**shared packs** (the pack directory itself is the knowledge root; its db is
pure knowledge, late-bound at query time). Both are scaffolded from the *same*
template (`init` / `init --pack`), so organisational alignment is a mechanical
fact, not a convention.

### One ruler: scope of concern

Documents are organized on a **scope-of-concern ladder** (largest → smallest),
declared by a YAML frontmatter field:

| Tier | frontmatter | Scope | `--scope` |
|------|-------------|-------|-----------|
| Architecture | `architecture:` | the whole project | `overview` |
| Domain | `domain:` | a cross-module functional area | `overview` |
| Module | `module:` | one code unit / folder | `unit` |
| Feature | `feature:` + `module:` | one atomic thing (an ability, an algorithm) | `unit` |
| (Detail) | inline `###` | inside a document | `detail` |

Two placement judgements matter more than the ladder itself:

- **A feature doc earns a standalone file when the knowledge is *worth
  querying on its own*** — three lines of core formula may deserve
  independence while a 30-line class table belongs in a `###` subsection.
  Standalone files get their own retrieval entry and evolve independently.
- **Lessons sit off the ladder.** An error is cross-cutting, never "owned".
  Its schema-enforced shape is Symptom → Root Cause → Fix → Guard, and the
  Symptom carries the *verbatim* error text — that is the anchor the next
  agent's query will hit. The document names the error, never the fix.

### The parsing contract

In one sentence: **title keywords decide semantics, bullets decide claims,
backticks decide code anchors, and body length decides indexability.**

- **Section kind by title keyword** (`classify_kind`): `flow` → data_flow,
  `responsib` → responsibility, `claim` → design_decision, `boundar` →
  boundary, `evidence` → evidence, `struct` → data_structure… A casually
  named title silently falls back to the semantics-less `section` kind and
  loses precise ranking.
- **Claims by bullets**: in a section titled with `claim`/`boundar`, every
  `- ` bullet becomes one claim; `[extracted]` / `[inferred]` prefixes mark
  credibility (mechanically verifiable fact vs semantic judgement).
- **Anchors by backticks**: `## Evidence` bullets use the strict
  `` `symbol` defined at `path:line` `` form, checked against the code index —
  a mismatch is `⚠ drift`, the lever that keeps documents fresh. Backticked
  and bare CamelCase symbols elsewhere become mentions, kept only when
  resolvable (the noise gate).
- **Indexability by body length**: under 30 substantive characters → degraded;
  an empty heading → quarantined out of retrieval.
- **Lessons declare applicability**: optional `applies-when` / `excludes`
  context slugs and a `guard-strength` (`directive` → `scope` → `hint` →
  `reference`) persist on every unit of a lesson doc and are matched
  *exactly* against a declared `query --context` — never guessed from query
  text. Without a declared context the packet simply discloses the contract
  (`applies-when: … · excludes: …`) and the agent judges its own situation.
  The packet of a lesson hit always surfaces the Guard block with its
  strength semantics.

### Context Envelope and layered granularity

Each document is split into self-contained **Knowledge Units** along its ATX
heading tree (a `#` inside fenced code is never a heading). Every unit
carries a **Context Envelope** — `heading_path` ancestry such as `Weapons >
Weapons Module > Data Flow` — a `parent_id`, and a scope: the root's comes
from the frontmatter tier, internal sections' from tree depth (`##` = section,
`###`+ = subsection). A mechanical **file tier** (derived from the code layer
at compile time, never authored) bridges module docs and symbols. Retrieval
is intent-layered: `query --scope overview|unit|section|detail` routes a
question to the right granularity.

### Quality is gated, not hoped for

Three auditable gates stand between a document and the retrieval index:

1. **`lint`** — the pre-compile hard gate: frontmatter/tier/heading/evidence
   format rules, all named; errors exit non-zero (CI- or pre-commit-ready).
2. **Chunk Contract** — the pre-index admission gate: `empty-leaf`
   (quarantine), `thin-content`, `unclear-reference` (pronoun-only claims),
   `unresolved-mention`, `missing-boundaries` (degrade). `alexandria contract`
   reports the pass rate and every violation with its rule, reason, and
   location.
3. **Tier schema** — required section kinds per tier, matched by kind keyword
   (never exact titles), checked by both `lint` and `compile`; evolution is
   additive-only, so old documents never silently break.

`missing-boundaries` encodes a core belief: **knowledge must state what it
does *not* answer**, so local conclusions are never over-generalised.

### The maintenance loop

After editing docs, verify *with the engine*, not by feel:

```
lint → compile → contract (gate) → refs (drift) → query "<target question>" (answerability)
```

Document compilation is a full rebuild (~1s at this scale) and stays decoupled
from the incremental `scan` — editing docs never needs a re-scan.
`alexandria scaffold <code dir>` derives a module-doc draft from the code
index (real classes, dependencies, consumers, evidence pre-filled): the
machine writes the structure, you write the semantics.

**Authoring is a spec, not a guess.** See
[`AUTHORING.md`](AUTHORING.md) for the full contract: document skeletons, the
heading-keyword → semantic-kind table, and the change checklist. Every rule
there is derived from the engine's actual parsing behavior with code
references.

---

## How retrieval works

`query` fuses four independent recall routes with **Reciprocal Rank Fusion**, so
ranking stays explainable (each hit is tagged with the routes that surfaced it):

| Route | Signal | Recalls |
|-------|--------|---------|
| **bm25** | FTS5 full-text | natural-language relevance |
| **symbol** | exact code symbols in the query → reverse-lookup | precise, high-confidence |
| **graph** | 1-hop code-graph neighbors of the query's symbols | associative ("things around what you asked") |
| **vector** | cosine over per-library embeddings | morphological similarity by default; optional true neural similarity (synonyms, paraphrase) |

Each top hit is then assembled into a self-contained **Evidence Packet**:
ancestor context, full body, child units, claims/boundaries, layered evidence
(primary author-cited / supporting mentions / graph relations) **with inlined
source excerpts**, plus an `answerability` verdict
(`sufficient` / `partial` / `insufficient`) and a `recommended_action`
(`proceed_with_evidence` / `proceed_with_caveats` / `fallback_to_source`). The
goal: enough in one packet that the agent rarely needs a second round-trip.

---

## Vector / semantic search (optional, offline)

The vector lane is **morphological by default**: the built-in `hash-ngram`
embedder needs no model file and no network, captures word-shape similarity
(`cooling` ↔ `cooldown`), and keeps the single-binary build lean.

For true semantic similarity — synonyms, paraphrase, and answers that share no
surface words with the query — you can enable the **local neural embedder**
behind the same `Embedder` trait. It runs on the CPU, requires no network at
runtime, and is gated behind a Cargo feature so the default build pays
nothing for it.

### 1. Build with the `neural` feature

```bash
cargo build --release --features neural
```

### 2. Download a model locally

No automatic download is performed. Place a HuggingFace-style BERT encoder
checkout in your project (tested with `sentence-transformers/all-MiniLM-L6-v2`):

```bash
mkdir -p .alexandria/models/all-MiniLM-L6-v2
cd .alexandria/models/all-MiniLM-L6-v2

curl -O https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json
curl -O https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json
curl -O https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors
```

The model file is ~90 MB in FP32 (22.7 M parameters). Plan for ~150 MB of
runtime memory and ~8–12 seconds to embed a 1,000-document knowledge base on a
modern CPU (only changed documents are re-embedded after the first run).

### 3. Enable it in `alexandria.toml`

```toml
[vector]
enabled = true
embedder = "minilm-l6-v2"      # default is "hash-ngram"

[vector.neural]
model_dir = ".alexandria/models/all-MiniLM-L6-v2"   # resolved relative to project root
# max_tokens = 256                                   # embedding input budget; overflow is reported
```

### 4. Recompile and query

```bash
alexandria compile
alexandria query "prevent my weapon from overheating"
```

The new embeddings are tagged with the model id (`minilm-l6-v2`), so the
content-hash gate automatically re-embeds every unit once and never mixes
vectors from different models.

---

## Project layout

```text
alexandria/
├─ Cargo.toml
├─ alexandria.toml              # scanner / index / retrieval configuration
├─ ARCHITECTURE.md         # deep design doc (data flow, schema, pipelines, limits)
├─ TODO.md                 # tracked technical debt (e.g. decl/def resolution plan)
├─ knowledge/
│  ├─ AUTHORING.md         # knowledge-document authoring & maintenance spec
│  └─ docs/                # indexed Markdown knowledge (domains/ modules/ features/ lessons/)
├─ src/
│  ├─ main.rs              # command dispatch
│  ├─ cli.rs               # CLI (clap) definitions
│  ├─ config.rs            # alexandria.toml model + defaults
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
├─ .alexandria/                 # generated index (git-ignored)
└─ target/                 # build artifacts (git-ignored)
```

---

## SQLite index

Everything lands in one WAL-mode SQLite database (`.alexandria/index/alexandria.db`) with
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
is an approximation; the neural embedder is optional and requires a local model;
no MCP server yet). Tracked next steps live in [`TODO.md`](TODO.md), including
a clangd-inspired declaration/definition resolution plan and an MCP integration.

---

## License

MIT.
