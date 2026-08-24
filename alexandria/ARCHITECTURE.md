# Brain-RS Architecture

> A **project knowledge index and retrieval engine** for coding agents (Rust).
> Goal: compile a code repository into a searchable index of "structural code
> facts + hand-written knowledge units", so an agent can quickly learn — before
> making changes — "what this code is, which knowledge covers it, and what a
> change would affect".

---

## 0. Red lines (inviolable design constraints)

These constraints are deliberate; no change may break them:

1. **The default scanner depends on no compiler/build system**
   - No Clang / clangd / `compile_commands.json`
   - No Unreal build tools, UBT, or any project scripts
   - Only **read-only, compiler-independent, low-cost lexical scanning**
   - The price: structural facts have **boundaries** (see §7 Known Limits);
     they must be documented honestly, never disguised as semantic precision.

2. **No reading of legacy `.pi` configuration**
   - This engine is a clean rewrite with its own `brain.toml`; it never reads
     historical hard-coded configuration back in.

3. **Zero side effects on the target project**
   - Source code is only read; all artifacts are written to the project's own
     `.brain/` state directory (gitignored).

---

## 1. Data-flow overview

**One brain per project; one database per shared knowledge pack; the project
root gains a single `.brain/` entry.** The engine binary is shared. Each
project converges config, private knowledge, project-level packs, and the
generated index under `<project>/.brain/` (`brain.toml` + `knowledge/` +
`packs/` + `index/`; only `index/` is gitignored). Reusable ecosystem
knowledge exists as **packs** (shared knowledge bases) — `<engine>/packs/<name>/`
or `<project>/.brain/packs/<name>/` (project wins). Every pack has its own
index database: **one knowledge base = one database**, never any cross-talk.

```
 project side                              engine side (shared)
 ┌─ <project>/.brain/brain.toml ────────┐
 │  scan (code layer)  compile (private)│    compile --pack (shared)
 │   ▼                  ▼               │     ▼
 │  symbols/edges   nodes/claims/refs   │    nodes/claims/refs (unresolved)
 │   └──────┬───────────┘               │     │
 │          ▼                           ▼     ▼
 │   <project>/.brain/index/brain.db    packs/<name>/.brain/pack.db
 └──────────┬────────────────────────────┘
            ▼        query: multi-brain fan-out + global RRF fusion
     project brain + every enabled pack brain → hits labelled by brain
```

- **`scan`**: parallel, incremental scan of source into `symbols` / `edges` /
  `files` (only the project brain has a code layer).
- **`compile`**: splits project Markdown knowledge into Knowledge Units and
  extracts claims / evidence / symbol cross-references.
- **`compile --pack`**: compiles pack docs into the pack's own database; with
  no code layer, all symbol binding is **deferred** to query time.
- **Late binding**: pack evidence/mentions/claim verification resolve at query
  time against the **querying project's** code index — shared knowledge is
  code-agnostic; only bound to a concrete project does "right or wrong" mean
  anything.
- The two steps are decoupled: code structure changes often (incremental scan
  is fast), hand-written knowledge changes rarely (full rebuild is fine).

---

## 2. Database schema

A single SQLite file `.brain/index/brain.db`. Main-database PRAGMAs: `WAL` +
`synchronous=NORMAL` + `temp_store=MEMORY`.

| Table | Purpose | Key points |
|-------|---------|-----------|
| `files` | Incremental core: per-file `hash` / `mtime` / `size` | mtime+size fast-path for "unchanged"; hash as backstop |
| `symbols` | Code symbols (class/struct/function/…) | Auto-increment PK; indexes on name / qualified_name / file |
| `edges` | Dependency edges | File-level import/include + **symbol-level calls** |
| `nodes` | Knowledge Units (document sections) | `parent_id` tree, `heading_path` context envelope, `status` gate |
| `nodes_fts` | FTS5 full-text index (external content) | Triggers stay in sync with `nodes`; BM25 ranking |
| `claims` | Assertions / boundaries (bullets of Key Claims / Boundaries) | `kind` = claim / boundary; `source` + `verification` grading |
| `node_refs` | Document ↔ code symbol cross-references | `ref_kind` = evidence / mention; claimed vs resolved |
| `metadata` | Scan/compile timestamps, scanner mode | |

---

## 3. The scan pipeline (`scan`)

**Parallel extraction + sharded parallel writes + serial merge** — see
`scanner/mod.rs`:

1. **Serial walk**: `collect_candidates` walks the configured directories,
   filters by extension/excludes/size, and records each file's `mtime`/`size`.
2. **Fingerprint preload**: `load_known_files` reads `path → (hash, mtime,
   size)` into memory in one query.
3. **Sharded parallelism**: candidates are round-robined into `min(threads, 8)`
   shards; each rayon worker:
   - **Fast path**: `mtime + size` match the old fingerprint → unchanged,
     **the file is never read** (the core incremental win).
   - **Slow path**: read the file → BLAKE3 hash; identical hash (e.g. touch)
     still means unchanged.
   - **Changed**: run the regex extractors for symbols/edges and write to the
     worker's **private `shard_k.db`**.
4. **Serial merge**: `merge_shards` `ATTACH`es one shard at a time → delete
   stale rows → `INSERT ... SELECT` into the main database → `DETACH`.
   - One shard attached at a time (sidesteps SQLite's ATTACH limit of 10).
   - `INSERT` names columns explicitly and omits `id`, letting the main
     database re-autoincrement and avoiding cross-shard id collisions.
5. **Finish**: prune vanished files, update metadata, delete the shard dir.

**Why parallel writes are safe here**: SQLite's single-writer lock is per
database file. Workers each write their own shard file (independent
connections, independent locks) and never touch the main database;
`rusqlite::Connection` is not `Send`/`Sync`, so the compiler itself
guarantees a worker cannot misuse the main connection.

> Performance: Lyra (725 files / 4,546 symbols) full scan ≈ 2.4s, pure
> incremental ≈ 1.4s. At this scale writes are not the bottleneck; sharded
> parallel writes are a forward investment in knowledge-base growth.

---

## 4. Knowledge-unit splitting (`compile` → `split_into_units`)

Instead of "one node per document", each document is split along its **ATX
heading hierarchy** into self-contained Knowledge Units:

- A **document root** always exists (reusing the leading `# H1`, or
  synthesised from the file name) and carries the preamble plus orphan
  paragraphs.
- A heading stack maintains ancestry: `###` attaches to the nearest `##`.
- Each unit carries:
  - a **Context Envelope**: `heading_path`, e.g. `Weapons > Weapons Module >
    Data Flow`;
  - a **parent_id**: forming the in-document tree;
  - **fenced-code-block protection**: `#` inside ` ``` ` / `~~~` fences is
    never mistaken for a heading.

### The Chunk Contract (an auditable gate before indexing)

`evaluate_contract` is the gate every unit must pass before entering the
retrieval index. It is made of **named rules**; a failed rule produces a
`ContractViolation` with a reason (persisted to `contract_violations`) rather
than an opaque status:

- `empty-leaf` (severity=quarantine): a heading with no body and no children →
  quarantined, **excluded from retrieval**.
- `thin-content` (severity=degrade): fewer than 30 substantive body characters
  → degraded; retrievable but may be down-weighted.
- `missing-envelope` (severity=degrade): no `heading_path` context envelope →
  degraded.
- `unclear-reference` (severity=degrade): a claim/boundary bullet that opens
  with a bare pronoun ("It…", "This module…", 它…) and names no symbol —
  reference completeness: claims must name their subject.
- `unresolved-mention` (severity=degrade, project brains only): a backticked
  (author-intended) symbol mention that does not resolve in the code index —
  reference closure. Unresolved backtick mentions are now *stored*
  (resolved=0) instead of silently dropped; plain-text candidates stay
  noise-gated. Pack brains defer closure to query-time late binding.
- `missing-boundaries` (severity=degrade): a domain/module/feature document
  with no Boundaries section — boundary completeness: knowledge must state
  what it does *not* answer so local conclusions are never over-generalised.
- Structural headings (empty body but with children) serve organisation and
  pass as `accepted`.

The final status is the most severe violation (quarantine → quarantined, else
degrade → degraded, else accepted). Retrieval (`query`) returns only
`accepted` / `degraded`; quarantined units are excluded.

The gate is **auditable**: `brain contract` reports the pass rate and lists
every degraded/quarantined unit with the named rule it failed, the reason,
and the source location — transparent and reproducible.

---

## 5. Claims / Evidence / cross-references

Alongside splitting, structured extraction runs per unit:

- **Claims** (`claims` table): in a section whose title contains "Claim", every
  bullet becomes `kind=claim`; containing "Boundar" → `kind=boundary`.
  Assertions and boundaries become first-class rows.
- **Credibility grading** (two orthogonal axes):
  - `source`: `extracted` (a mechanically verifiable fact) vs `inferred` (a
    semantic judgment). Authors may mark bullets explicitly with an
    `[extracted]` / `[inferred]` prefix; unmarked claims carrying a
    `` `Sym` defined at `path:line` `` evidence binding count as extracted,
    the rest as inferred.
  - `verification`: the engine's check of a location binding — `verified`
    (claimed file matches the code-index resolution) / `drift` (resolves
    elsewhere) / `unresolved` (symbol gone) / `unverifiable` (no binding).
    Checked at compile time in the project brain; late-bound at query time in
    pack brains (see §1). The Evidence Packet's answerability treats verified
    extracted claims as the strongest grounding signal; drifted claims and
    claims marked extracted yet unverifiable raise warnings.
- **Evidence** (`node_refs`, `ref_kind=evidence`): parses
  `` `symbol` defined at `path:line` `` and records the document's **claimed**
  definition site (`claimed_file/line`) — kept even when unresolvable, to
  surface drift.
- **Mentions** (`node_refs`, `ref_kind=mention`): every backticked symbol in
  prose, resolved against the `symbols` table, **kept only when resolvable**
  (the noise gate), building "document section ↔ code definition" links.

### Drift detection (doc/code drift)

`refs` displays the document-authoritative `claimed` location for evidence;
when `claimed` disagrees with the engine's `resolved` location it prints
`⚠ drift`. (Historically, drift exposed a scanner defect: lexical scanning
recorded the forward declaration `class ULyraWeaponInstance;` as a definition
and mistook the UE export macro `LYRAGAME_API` for a class name, so `resolved`
picked the wrong file. Fixed in the scanner — forward declarations are no
longer definitions and export macros are skipped; symbol count dropped
4511 → 3190 and `ULyraHealthComponent` et al. now resolve to their true
definitions. Drift detection remains to catch future doc/code drift.)

---

## 5.5 Multi-route retrieval fusion (`query` · B4)

`query` is no longer single-route BM25 — it is **multi-brain fan-out + three
recall routes + Reciprocal Rank Fusion (RRF)**: the project brain and every
enabled pack brain each run the three routes independently; hits are labelled
with their `brain` of origin and fused globally by RRF. For packs, the
symbol/graph routes resolve symbols through the **project brain's** code index
and then reverse-look-up the pack's own `node_refs`. `locate` / `graph` query
only the project brain (the code layer lives there); `refs` / `contract` /
`status` report per brain.

| Route | Signal | Weight | What it recalls |
|-------|--------|--------|-----------------|
| **bm25** | FTS5 full-text + BM25 | 1.0 | Natural-language relevance (lexical) |
| **symbol** | Code symbols in the query → `node_refs` reverse lookup of citing units | 2.0 | Precise, high-confidence ("the sections about this symbol") |
| **graph** | Graph neighbours of the symbols → units referencing them | 0.6 | Associative recall ("things around what you asked") |
| **vector** | Cosine similarity over per-brain embeddings (`[vector]` config) | 0.8 | Morphological similarity (B8 — see the honest note below) |

- **The vector route (B8) is honest about its embedder.** The built-in
  `hash-ngram` embedder is a deterministic feature-hashing embedder (word
  uni/bigrams + char 4-grams, BLAKE3 bucketed): fully offline, zero
  dependencies — but *morphological, not neural*. It helps when a query and a
  document share word shapes (cooling ↔ cooldown); it cannot bridge true
  synonyms (memory ↔ brain), and substring coincidence can produce false
  friends (overheating ↔ override). Embeddings are stored per brain in
  `node_embeddings` (model+dim pinned), refreshed incrementally at `compile`
  via content-hash gating; a vector-only hit never boosts answerability. The
  `Embedder` trait is the plug point: a local neural model (e.g. MiniLM via
  Candle) can replace the default without touching retrieval.

- **Symbol candidates**: the query is mined with `mentioned_symbols`
  (multi-hump/underscore heuristics) and validated against the `symbols`
  table; a purely natural-language query yields none and degrades cleanly to
  BM25 only (no regression).
- **Graph two-hop bridging**: ① symbol-level call neighbours (function↔function
  edges); ② file-level — the symbol's defining file → include-neighbour files
  → their symbols. Because `edges.target_file` stores the raw `#include`
  literal (a partial path) while `symbols.file` is a full project-relative
  path, the two are bridged **by basename**.
- **RRF fusion**: `score(node) = Σ_route w / (K + rank)` with K=60. No need to
  normalise different routes' score scales — only ranks are used.
- **Provenance transparency**: every hit is labelled with the routes that
  surfaced it, `⟨bm25+symbol+graph⟩`, keeping ranking explainable and
  auditable (`--json` carries a `routes` field).

**Fusion in action** (measured on `query ULyraEquipmentManagerComponent`): the
top hit is Equipment, surfaced by all three routes; the AbilitySystem
knowledge units are recalled **solely** by `⟨graph⟩` — those documents contain
no query terms at all and neither BM25 nor symbol could find them; the
include graph's association pulled them in.

---

## 5.6 Layered-granularity nodes (`query --scope` · B5)

One document is split into knowledge units of different granularity. The
**document root's** scope is declared by the frontmatter tier field (the
scope-of-concern ladder); **internal sections** take their scope from tree
depth:

| scope | Tier | Source |
|-------|------|--------|
| `project` | Architecture (whole project) | doc root + frontmatter `architecture:` |
| `domain` | Domain (cross-module area) | doc root + frontmatter `domain:` (alias `system:`, e.g. Combat) |
| `module` | Module (single code unit) | doc root + frontmatter `module:` (default) |
| `feature` | Feature (atomic thing) | doc root + frontmatter `feature:` (+ `module:` ownership) |
| `file` | File (one source file) | **mechanical** — derived from the code layer at compile time |
| `section` | Major sections | direct children of the doc root (tree depth 1, usually `##`) |
| `subsection` | Detail | deeper nested nodes (tree depth ≥2, `###`+) |

- **Root scope by frontmatter tier, internal scope by tree depth**: internal
  sections are stable whether the document starts at `#` or `##`
  (`depth = number of ancestors`).
- **The `file` tier is mechanical, not authored**: `compile` derives one node
  per source file that defines ≥1 symbol (`id=file:<path>`, kind=`file`), with
  a generated symbols/includes body and an evidence ref per defined symbol
  (claimed = resolved, so verification is trivially `verified`). File nodes
  bridge module docs and symbols — "what does this file do" is now
  answerable, and they exist only in brains with a code layer (never in
  packs). The answerability gate relaxes the authored-claims requirement for
  them (a mechanical node has none by construction).
- **Intent-layered retrieval**: `query --scope <overview|unit|section|detail|all>`
  routes different granularity needs to the corresponding scopes:
  - `overview` → `project` + `domain` (the big picture)
  - `unit` → `module` + `feature` + `file` (one concrete unit/thing/source file)
  - `section` → `section` (major sections)
  - `detail` → `subsection` (deep detail)
  - `all` (default) → no filter
- **Implementation**: fusion over-fetches to `max_results×4`, the scope filter
  is applied at fetch time, and results truncate to `max_results`; route SQL
  is unchanged.

**Measured** (same `query "weapon damage combat"`): `--scope overview` returns
only the domain node (Combat System); `--scope unit` only module/feature nodes
(Weapons Module); `--scope detail` only deep-detail nodes like Class
Responsibilities — one query, several granularities, each to its purpose.

---

## 5.7 The single-round self-contained Evidence Packet (saving agent turns)

**Motivation**: the engine takes 40–110ms per invocation (cold process),
which is negligible; what is expensive is the agent's **interaction turns** —
each turn wraps a seconds-long LLM round-trip. So the optimisation target is
not "a faster engine" but "**one `query` = one complete decision unit**":
maximum information density to collapse multiple turns into one.

Two of the most common redundant turns were eliminated:

1. **Packets assembled by default** (previously the default was a summary
   list, forcing a second `--assemble` turn). Now `query` returns the top-3
   full Evidence Packets by default; `--brief` falls back to a lightweight
   list for quick exploration.
2. **Inlined source excerpts** (previously, insufficient answerability meant
   `fallback_to_source`, and the agent spent another turn reading
   `file:line`). During assembly, a source window around every evidence
   reference's `resolved_file:line` (1 line before + 5 after, with line
   numbers) is **read into the packet**. Even when document knowledge is
   insufficient, the agent gets "what the doc says + what the source actually
   is" in the same turn — no further file reads.
   - Budget: at most 6 symbols per packet, primary evidence first, then
     supporting; a per-file line cache avoids re-reads.
   - Unreadable files (deleted/moved) → empty excerpt, itself a drift signal,
     not an error.

**Cost**: default assembly + reading 6 source files raised one query from
45ms to **58ms** (+17ms) — still far below one LLM round-trip.

**Effect**: one effective knowledge fetch went from "typically 2 turns, 3–4
when verification is needed" to **ideally 1 turn** — the packet arrives with
"answer + self-assessment (answerability) + layered evidence + inlined source
+ recommended action"; `sufficient` can be used directly, and `partial` can be
checked on the spot against the inlined source.

---

## 6. CLI commands

| Command | What it does |
|---------|--------------|
| `init` | Scaffold the knowledge-root template (project: `.brain/brain.toml` + `.brain/knowledge/`; `--pack <dir>`: pack root). Projects and packs share one template source; idempotent, never overwrites |
| `scaffold <dir>` | Derive a module doc draft from the code index (real classes/deps/consumers/evidence pre-filled) → `.brain/knowledge/modules/<Name>.md`; generation-layer bridge, never overwrites |
| `scan` | Parallel incremental source scan → symbols / edges / files |
| `compile` | Compile project knowledge docs → Knowledge Units / claims / node_refs; `--pack <dir>` compiles a shared pack into `<pack>/.brain/pack.db` |
| `query <text>` | **Four-route fused retrieval** (BM25 + symbol + graph + vector, RRF) across all brains; **top-3 self-contained Evidence Packets by default (with inlined source)**; `--brief` for a lightweight list; `--scope <overview\|unit\|section\|detail\|all>` for granularity |
| `locate <symbol>` | Locate a code symbol's definition site (project brain only) |
| `refs <symbol>` | **Reverse lookup**: which knowledge units reference this symbol (evidence/mention/drift), across all brains |
| `graph <kind> <symbol>` | Graph queries: callers/callees (symbol-level calls, multi-hop), deps/dependents (file-level includes), impact |
| `status` | Index statistics (per-table counts, gate grades, timestamps, enabled packs) |
| `contract` | **Chunk Contract audit**, per brain: degraded/quarantined units with the named rule, reason and location |
| `lint` | **Pre-compile hard gate**: document format / directory layout / pack-reference rules (named, severitised); exits non-zero on errors; `--pack <dir>` lints one pack |
| `feedback` | **Answer-feedback records** (project brain only): the agent records verdicts (`useful`/`partial`/`wrong`/`stale`) on the user's behalf; latest non-useful verdict surfaces as a packet warning until cleared; `status` shows the verdict histogram |

Global flags: `--project-root`, `--config`, `--state-dir`, `--format`
(`text` default for humans · `json` for machines · `tagged` for LLM agents,
rendered as XML-ish semantic tags with CDATA payloads — explicit field
boundaries, zero escaping); per-command `--json` ≡ `--format json`. `tagged`
is supported on `query` / `refs` / `locate` / `graph`. Every Evidence Packet
carries `node_id` + `brain`, the address `feedback` targets.

---

## 7. Known limits (honestly documented, not hidden defects)

1. **Class/struct forward declarations, export macros, and function
   decl/def roles are handled**: `class Foo;` forward declarations are not
   recorded as definitions; UE export macros are skipped; functions carry a
   `role` (`definition` | `declaration`, lexical
   `isThisDeclarationADefinition`), and every symbol lookup prefers
   definitions (`locate` tags declarations `[decl]`). Out-of-line
   definitions record their qualified name (`UWeapon::Fire`) as a weak
   fingerprint. Remaining ceiling (no compiler): multi-line signatures fall
   back to `declaration`, and same-named overloads still share one
   name-keyed resolution — Evidence claimed locations stay more
   trustworthy, and drift warns.
2. **Call edges are a lexical approximation**: function scope is tracked by
   brace depth and can be disturbed by braces inside strings / block comments
   / macros / lambdas; callees are recorded by name only, not resolved to a
   defining file (same-named methods on different classes are
   indistinguishable). Common UE macros (TEXT/LOCTEXT/UE_LOG/check/ensure…)
   are filtered; some noise remains. `graph callers/callees` is usable;
   Python uses indentation-based scopes and produces no call edges for now.
3. **Edges are stored as name strings, not symbol-id foreign keys**:
   cross-file association is lexically approximate, not semantically exact.
4. **The vector route is morphological, not neural**: B8 shipped with the
   offline `hash-ngram` embedder — word-shape similarity only, with known
   blind spots (true synonyms) and false friends (substring coincidence). A
   neural embedder can be plugged behind the `Embedder` trait later; the
   graph route's yield stays bounded by edge quality (see §2).
5. **No MCP / agent integration**: this is a CLI engine today.

These boundaries are the direct price of the §0 red lines (no compiler) —
**known and accepted** trade-offs.

---

## 8. Directory layout

```
alexandria/
├─ brain.toml            # engine default config: scan / index / retrieval
├─ packs/                # engine-level shared knowledge packs
│  └─ ue-lyra/           #   docs directly at the pack root + .brain/pack.db
├─ AUTHORING.md          # the knowledge-base maintenance spec (AI-facing)
├─ src/
│  ├─ main.rs            # command dispatch
│  ├─ cli.rs             # argument & subcommand definitions
│  ├─ config.rs          # config loading & normalisation
│  ├─ model.rs           # Symbol / Edge / retrieval result structs
│  ├─ init.rs            # knowledge-root scaffolding (shared template)
│  ├─ scanner/           # scan pipeline (mod) + per-language modules
│  │  ├─ mod.rs          #   parallel incremental scan + sharded writes + LanguageScanner trait
│  │  ├─ common.rs       #   shared: symbol building, call-noise filter, brace-scope state machine
│  │  ├─ cpp.rs          #   C++: class/struct/func + include + call
│  │  ├─ typescript.rs   #   TS/JS: func/class/import + call
│  │  └─ python.rs       #   Python: def/class + import
│  ├─ storage.rs         # database schema (main + shard DBs), paths
│  ├─ index/             # knowledge compile & retrieval
│  │  ├─ mod.rs          #   compile orchestration, knowledge sources (brains)
│  │  ├─ chunk.rs        #   document → Knowledge Unit splitting, frontmatter
│  │  ├─ contract.rs     #   Chunk Contract gate & audit
│  │  ├─ extract.rs      #   claims/evidence/symbol extraction
│  │  ├─ retrieve.rs     #   multi-brain multi-route fusion, refs/status
│  │  └─ packet.rs       #   Evidence Packet assembly & rendering
│  └─ graph.rs           # graph queries (symbol-level calls / file-level includes)
└─ .brain/index/brain.db # artifact (gitignored)
```
