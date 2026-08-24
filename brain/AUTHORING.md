# Knowledge-Base Maintenance Spec (Authoring Guide)

> **The primary reader of this spec is an AI (the agent maintaining the
> knowledge base); human contributors come second.** It lives at the engine
> repository root and is **not indexed by the engine** (the engine only scans
> the knowledge roots configured in `docs_dirs` and the shared packs enabled
> via `enabled_packs`). Every rule is derived from the engine's actual parsing
> behaviour with a code reference — just follow it; no guessing required.

Knowledge documents are the engine's **fuel and its only authoritative
source**: what the code yields is "mechanical facts" (symbols / calls /
dependencies), while "why it is designed this way, where the boundaries are,
and the end-to-end flows" can only come from knowledge documents. Their
quality directly determines whether the answers an agent retrieves are useful.

---

## The knowledge-base architecture (build the mental model first)

**Core concept: the knowledge root.** A knowledge root = one directory of
knowledge documents, with **documents directly at the root**, organised by a
four-tier template:

```
<knowledge root>/
  Architecture.md     ← L0 architecture (frontmatter: architecture:) entry view
  domains/            ← L1 domains (domain:) cross-module end-to-end flows
  modules/            ← L2 modules (module:) single-code-unit responsibilities
  features/           ← L3 features (feature: + module:) atomic key things
```

**A complete knowledge base = one knowledge root + one index database.** There
are exactly two kinds of knowledge base in the system:

| | Project brain | Shared pack |
|---|---|---|
| Knowledge-root location | `<project>/.brain/knowledge/` | The pack directory itself (`<engine>/packs/<name>/` or `<project>/.brain/packs/<name>/`) |
| Index database | `<project>/.brain/index/brain.db` (knowledge + code layer symbols/edges) | `<pack>/.brain/pack.db` (pure knowledge, no code layer) |
| Built by | `brain-rs --project-root <project> compile` | `brain-rs compile --pack <pack dir>` |
| Symbol binding | Verified at compile time against this project's code index | **Late binding**: resolved at query time against the *querying* project's code index |
| Visibility | This project only | Engine-level: any project may enable it; project-level: this project only, and wins over a same-named engine pack |
| Enabled via | Automatic (the project brain always participates) | `enabled_packs = ["<name>"]` in the project's `brain.toml` |

**Scaffolding (never hand-create the directories)**: project and pack
knowledge roots are generated from **the same template**, so organisational
alignment is a mechanical fact, not a convention:

```
brain-rs --project-root <project> init      # .brain/brain.toml + .brain/knowledge/ template
brain-rs init --pack <pack dir>             # the same knowledge-root template in the pack dir
```

`init` is idempotent and never overwrites existing files. The template draft
itself passes the Chunk Contract (every section ≥ 30 characters) and contains
**no backticked symbols** (so the first compile produces no bogus
claims/evidence).

**There are only three maintenance actions** (details in §5):

```
# 1. Changed project knowledge → rebuild the project brain
brain-rs --project-root <project> compile
# 2. Changed pack knowledge    → rebuild the pack database
brain-rs compile --pack <pack dir>
# 3. Always self-check after changes → gate / drift / answerability
#    (§5.2, the mandatory feedback loop for AI maintainers)
brain-rs [--project-root <project>] contract | refs <symbol> | query "<target question>"
```

The rest of this spec answers: **which tier to write at** (§1, granularity),
**how to write** (§2–§4, creating), and **how to change** (§5, the
maintenance loop).

---

## 0. The engine parsing contract (hard constraints, quick reference)

Before writing a document you must know how the engine **actually parses**
your Markdown. Every row here is a hard constraint.

| Element | Engine behaviour | Code reference |
|---------|------------------|----------------|
| **Frontmatter** | The first non-empty line must be `---`; ends at the next `---`; only simple `key: value` lines are read | `chunk.rs::detect_frontmatter` / `parse_frontmatter` |
| Tier fields | `architecture:` → root scope `project`; `domain:` (alias `system:`) → `domain`; `feature:` → `feature`; `module:` → `module` (default) | `compile_documents` |
| `module:` value | The last path segment is taken (`LyraGame/Weapons` → `Weapons`) as the Context Envelope's module identity | `compile_documents` |
| Other keys (tags/source/feature-slug) | **Ignored without error** — purely for humans | `parse_frontmatter` |
| **ATX headings** | `#`–`######`; the `#` **must be followed by a space** (`#tag` is not a heading) | `chunk.rs::parse_heading` |
| Heading hierarchy | Tree building: `###` attaches to the nearest `##`; the doc root's scope comes from the frontmatter tier field; `##` = section, `###`+ = subsection | `split_into_units` |
| Fenced code blocks | `#` inside ` ``` ` / `~~~` fences is never mistaken for a heading | `split_into_units` |
| **Section kind** | The semantic type is decided by **title keywords** (see §4) | `extract.rs::classify_kind` |
| **Claims** | In a section whose title contains `claim` or `boundar`, every `- `/`* ` bullet becomes one claim (wrapped continuation lines join their bullet) | `classify_claim_section` + `extract.rs::bullets` |
| **Claim credibility markers** | Bullet prefix `[extracted]` = mechanically verifiable fact / `[inferred]` = semantic judgment (case-insensitive, stripped before storage); without a marker, a claim carrying a `defined at` binding counts as extracted | `extract.rs::parse_claim_marker` + `compile_documents` |
| **Evidence** | In a section titled exactly `Evidence`, every bullet parses `` `symbol` ... `path:line` `` into a primary evidence binding | `compile_documents` |
| **Symbol mentions** | Backticked symbols + plain CamelCase/snake_case in all other sections, **kept only when resolvable in code** | `extract.rs::mentioned_symbols` |
| **The gate** | Empty section → quarantined (excluded); <30 substantive chars / no envelope / pronoun-only claim → degraded; unresolved backticked symbol / missing Boundaries → degraded | `contract.rs::evaluate_contract` + `compile_documents` |

**In one sentence**: title keywords decide semantics, bullets decide
claims/evidence, backticks + camelCase decide code anchors, and body length
decides indexability.

---

## 1. Where things go: four tiers by "scope of concern"

> The knowledge root's four-tier directory template (`Architecture.md` +
> `domains/` + `modules/` + `features/`) is defined in the **knowledge-base
> architecture** section above and is shared by projects and packs. This
> section answers: which tier should a specific piece of knowledge **be
> written into**.

The engine distinguishes granularity by `scope` (`query --scope`). Document
organisation uses **one ruler** — **scope of concern**: how large an area does
this knowledge govern? Four tiers, largest to smallest, plus an inline tier.

| Tier | Name | Scope of concern | Span | frontmatter | Directory |
|------|------|------------------|------|-------------|-----------|
| L0 | **Architecture** | The whole project | Everything | `architecture:` | knowledge root |
| L1 | **Domain** | One functional domain | Multiple code units | `domain:` | `domains/` |
| L2 | **Module** | One code unit | One folder | `module:` | `modules/` |
| L3 | **Feature** | One atomic thing | Single owner | `feature:` (+`module:`) | `features/` |
| — | Detail | Inside a document | — | inline `###` | (with the host doc) |
| — | **File** | One source file | — | **mechanical** (never authored — derived from the code layer at `compile`) | — |

**Key premise**: the engine assigns internal sections their `scope` by **tree
depth** (`##` = section, `###`+ = subsection), while the **document root's**
scope is decided by the frontmatter tier field (`chunk.rs` +
`compile_documents`). So "which tier" = "which frontmatter field" — directly
controllable.

### 1.0 Architecture documents (`architecture:`)
- **What to write**: the entry view of the whole project — technology stack,
  top-level layout, core conventions, module map. Answers "what is this
  codebase and how is it organised".
- **Typical**: `Architecture.md` (one per project, the agent's first stop).
- **frontmatter**: `architecture: <ProjectName>`.
- **Placement**: the knowledge root.
- **Optional**: small projects may skip it; large projects should strongly
  consider it — it is where an agent builds its global mental model.

### 1.1 Domain documents (`domain:`)
- **What to write**: a cross-module view of one **functional domain** —
  usually an end-to-end flow. Answers "how does <domain> work across the
  codebase".
- **Typical**: `Combat.md` (weapon fire → damage → health, spanning
  Weapons/AbilitySystem/Character); `Networking.md`.
- **Character**: defines no classes of its own; it references classes owned by
  other modules and strings them into a flow line (**no single code owner**).
- **frontmatter**: `domain: <DomainName>`; do **not** write `module:`.
- **Placement**: `domains/`.
- **Why "domain" and not "system"**: "system" is taken by runtime concepts
  (ability system, input system…) in game development and would be ambiguous;
  "domain" specifically means a functional division — no collision.

### 1.2 Module documents (`module:`)
- **What to write**: one **code unit's** responsibilities, architecture, data
  flow, and boundaries. Answers "what does <this folder> do and how is it
  organised".
- **Typical**: `Weapons.md` / `Character.md` (one per
  `Source/LyraGame/<X>/` directory).
- **frontmatter**: `module: LyraGame/<ModuleName>`.
- **Placement**: `modules/`.
- **Note**: "module" here means **a cohesive code unit/folder**, **not** a UE
  Build module (`.Build.cs`). Lyra's Weapons is a subfolder inside the
  `LyraGame` UE module.

### 1.3 Feature documents (`feature:` + `module:`, **standalone file**)
- **What to write**: one **atomic, concrete, key thing** — an ability, a
  scoring algorithm, a protocol, a complex state machine. Detailed,
  independently evolving, worth retrieving on its own. **It is the smallest
  independent unit** (hence "feature", not "topic" — a topic implies a broad
  theme, the opposite of atomicity).
- **Typical**: `HeroDash.md` (the dash ability's Task orchestration),
  `EliminationScoring.md` (the elimination scoring algorithm).
- **frontmatter**: `feature: <feature-slug>` + `module:
  LyraGame/<OwningModule>` (`module:` declares ownership and builds the
  Context Envelope identity).
- **Placement**: `features/`.
- **Why a standalone file instead of a `###` inside the module doc**: the
  content volume would bloat the module doc; it evolves independently of the
  rest of the module; and it deserves a complete Context Envelope plus its
  own Evidence and retrieval entry.
- **Measured**: querying "elimination scoring streak bonus" hit this feature
  doc in all Top-3 slots — a standalone file loses no retrieval and instead
  gives this key knowledge a dedicated entrance.

### 1.4 Inline detail (`###` subsections, **never standalone**)
- **What to write**: details that **depend on the host document and are not
  worth retrieving independently** — class responsibility tables, struct
  lists, short explanations.
- **Placement**: as `###` third-level subsections inside module/feature docs
  (e.g. `### Class Responsibilities`).
- **Why**: such detail is meaningless out of context; the engine
  automatically marks it `subsection` by tree depth, and `--scope detail`
  finds it.

### Choosing between 1.3 and 1.4 (the key judgement)

> **Ask one question: "Will anyone query this piece of knowledge *on its
> own*?"**
> - Yes (scoring algorithm, dash ability, core formula) → **§1.3 standalone
>   feature file**.
> - No, it is only a component of its host (class responsibility table) →
>   **§1.4 inline `###`**.
>
> The criterion is "**worth independent retrieval**", **not content length** —
> 3 lines of core formula may deserve independence, while a 30-line class
> table may only deserve a `###`.

### 1.5 Cross-module orchestration features (complex abilities / multi-Task skills)

**Scenario**: a complex ability orchestrates multiple Tasks (locomotion /
animation / camera Tasks each owned by different modules), and the reader
wants to see "how control jumps between Tasks". This knowledge **spans
modules**, which makes its tier confusing.

**First, break a misconception**: "spanning several modules" is **not** the
deciding axis — nearly every ability touches animation/locomotion/camera. The
real axes are:
1. **Is there a single owner?** The ability has one GA class defining it; the
   Tasks it calls are **collaborators/dependencies**, not identity.
2. **What is the reader querying?** "How does this ability orchestrate Tasks"
   — the value is in the **orchestration flow** (how Task A→B→C hand off,
   what each waits for, where control jumps on interruption).

**Conclusion: it is a §1.3 feature document** (orchestration type), with
identity owned by the module defining the ability:
- frontmatter: `feature: <ability-slug>` + `module: LyraGame/<GA's module>`;
- the body is a **Task Orchestration Flow** (`## Data Flow`): use the **real
  class names of each Task** in the diagram (cross-module is fine — the
  engine resolves them into code anchors);
- **Edge Cases** spell out where control jumps on interruption/branching —
  the soul of this kind of document;
- Evidence references core symbols from each collaborating module, building
  **cross-module anchors**.

**The key mechanism (why you don't cram every module into one doc)**: at
retrieval time, B4's **graph route** follows the ability's symbols and
**automatically stitches the collaborating modules' Task knowledge into the
results**. You write just this one orchestration doc with real class names;
"how does the locomotion Task work" / "how does the camera cut" get filled in
by each module's own docs.

> **Measured**: one orchestration doc spanning AbilitySystem/Character/Camera,
> queried by symbol, returned both the orchestration doc (all three routes,
> `⟨bm25+symbol+graph⟩`) **and** the Character module's Task nodes — one
> orchestration doc + the graph route assembled the cross-module picture
> automatically. **No** mega-document needed.

**When to use `domain:` instead**: when what you write is **not one concrete
ability** but "**the common pattern of all dash-like abilities**" — a
cross-cutting view with no single owner. That is a domain-level pattern, not
a feature.

### Ownership decision quick reference

| What you are writing | Tier | frontmatter |
|----------------------|------|-------------|
| The project's entry/layout/conventions | Architecture | `architecture:` |
| One cross-module flow (no single owner) | Domain | `domain:` |
| One module's overall responsibilities/architecture | Module | `module:` |
| One independently queryable atomic thing (algorithm/ability/protocol) | Feature | `feature:` + `module:` (ownership) |
| **A complex ability's Task orchestration (cross-module but single GA owner)** | **Feature** | **`feature:` + `module:` (GA's module)** |
| A not-independently-queryable component of a host doc (class table) | Inline `###` | — (with the host) |

### Retrieval granularity mapping (`query --scope`)

Scope tiers map one-to-one onto retrieval granularity filters:

| `--scope` | Tiers hit | Intent |
|-----------|-----------|--------|
| `overview` | project + domain | "Give me the big picture" — architecture and domains |
| `unit` | module + feature + file | "Give me one concrete unit/thing/source file" |
| `section` | `##` sections inside docs | Major sections |
| `detail` | `###` subsections inside docs | Deep detail |
| `all` (default) | no filter | Everything |

### Recommended directory layout (shared by project knowledge roots and pack roots; documents directly at the root; generate with `brain-rs init`, never hand-create)
```
knowledge/              ← a knowledge root (shown relative to the root; project default docs_dirs is [".brain/knowledge"])
  Architecture.md       ← L0 architecture (architecture:) project entry
  domains/              ← L1 domains (domain:) cross-module flows
    Combat.md
    Networking.md
  modules/              ← L2 modules (module:) single code units
    Weapons.md
    Character.md
  features/             ← L3 features (feature: + module:) atomic things
    HeroDash.md
    EliminationScoring.md
```
> The engine scans knowledge roots **recursively** with `walkdir` —
> subdirectories work with zero configuration; hidden directories (like
> `.brain`) are skipped automatically.
> `system:` is still parsed as a **backward-compatible alias** of `domain:` —
> old documents keep working, but new documents must use `domain:`.
> Shared packs work the same way: `packs/<name>/` holds `Architecture.md` /
> `domains/` / `modules/` / `features/` directly at its root.


## 2. The standard document skeleton (module-level template)

Start every new module document **from this skeleton** — or faster, from a
machine draft: `brain-rs scaffold <code dir>` pre-fills real classes,
dependencies, consumers and evidence locations from the code index (structure
from the machine, semantics left to you). Every `##` title is carefully
worded to trigger the right kind.

````markdown
---
module: LyraGame/<ModuleName>
tags: [<human-readable keywords, ignored by the engine>]
source: manual
---

# <ModuleName> Module

<One-sentence overview: what this module provides. This paragraph becomes the
document root's summary.>

## Context

- **Module path:** `Source/LyraGame/<ModuleName>/`
- **Dependencies:** <other modules this relies on>
- **Consumers:** <who relies on this module>

## Architecture

```
<class inheritance/composition diagram, in monospace>
```

### Class Responsibilities

| Class | Parent | Role |
|-------|--------|------|
| `UFooClass` | `UBarBase` | <one-sentence responsibility> |

### Key Structs

| Struct | Usage |
|--------|-------|
| `FFooData` | <purpose> |

## Data Flow

```
<end-to-end flow diagram, nodes use real class/function names (CamelCase)>
```

## Key Claims

- [extracted] `USymbol` is defined at `Source/LyraGame/<ModuleName>/<File>.h:<line>` and <a mechanically verifiable fact>.
- [inferred] <a semantic judgment grounded in multiple code sites; one bullet each, independently quotable>.

## Boundaries

- This module does **not** <what it explicitly does not do>.
- <boundaries/limits — help the agent decide "if it's not here, stop looking">.

## Evidence

- `USymbol` defined at `Source/LyraGame/<ModuleName>/<File>.h:<line>`
- <one line per core symbol, strict format: `symbol` + `path:line`>
````

Domain-level documents (`domain:`) are isomorphic, with these differences:
- frontmatter uses `domain:` (no `module:`);
- the overview emphasises "which modules this spans";
- **Data Flow is the core** (the flow is the domain document's reason to
  exist);
- Evidence references symbols owned by **other modules** (cross-module
  anchors).

### The feature document template (§1.3)

Feature documents are **freer** than module documents: the point is to cover
one atomic thing thoroughly, so sections are organised by content — not every
standard section is required. But **the identity, boundaries, and evidence
sections are mandatory**.

````markdown
---
feature: <feature-slug, e.g. elimination-scoring>
module: LyraGame/<OwningModule>
tags: [<human-readable keywords>]
source: manual
---

# <Feature Name>

<One sentence: what this is and why it is a standalone file (key and
independently evolving). Becomes the document root summary.>

## Context

- **Owning module:** `Source/LyraGame/<OwningModule>/`
- **Trigger / Inputs:** <what triggers it / what comes in>
- **Consumers:** <who consumes its output>

## <Body, e.g. Algorithm / Protocol / State Machine / Task Orchestration Flow>

<Cover the algorithm/protocol/state machine/orchestration thoroughly. Formulas
in code blocks; deep branches in ### (naturally subsections).>

## Edge Cases

- <boundary conditions, special branches — the real value of this knowledge
  often lives here>.

## Boundaries

- This <feature> does **not** cover <what it explicitly ignores>.

## Evidence

- `USymbol` defined at `Source/LyraGame/<OwningModule>/<File>.h:<line>`
````

Key points:
- **`feature:` sets the root scope to `feature`** (`query --scope unit` can
  hit it); **`module:` declares ownership** (building the Context Envelope
  identity). Write both.
- A body-section title outside the §4 keyword table falls back to the generic
  `section` kind — **acceptable** (feature bodies are custom content), but for
  precise ranking give body subsections keyword-bearing names (e.g. call the
  orchestration flow `## Data Flow`).
- **Edge Cases / Boundaries are the soul of a feature doc**: half the value
  of an algorithm/ability lives in its boundary conditions — write them fully.

---

## 3. How to write each standard section (mapped to engine extraction)

| Section title | Triggers kind | What the engine extracts | Writing points |
|---------------|---------------|--------------------------|----------------|
| `## Context` | context | symbol mentions | List paths/dependencies/consumers; wrap paths in backticks |
| `## Architecture` | architecture | symbol mentions | Class relationship diagrams; class names in backticks or bare CamelCase both extract |
| `### Class Responsibilities` | responsibility | symbol mentions | Table; backtick the `Class` column → code anchors |
| `### Key Structs` | data_structure | symbol mentions | Same as above |
| `## Data Flow` | data_flow | symbol mentions | Class/function names in the diagram extract as anchors (**bare CamelCase works, backticks not required**) |
| `## Key Claims` | design_decision | **claims** (one per bullet) | Each claim self-contained and independently quotable — **name the subject**, no pronoun-only claims; mark credibility with `[extracted]` / `[inferred]` prefixes (§0); backticked symbols must resolve in code (`unresolved-mention` rule) |
| `## Boundaries` | boundary | **boundary claims** | **Mandatory** in domain/module/feature docs (`missing-boundaries` rule); use the "does **not**" phrasing and name the subject — "The Weapons module does **not**…", never a bare "It does **not**…" (`unclear-reference` rule) |
| `## Evidence` | evidence | **primary evidence bindings** | Strict format: `` `symbol` defined at `path:line` `` |

**Key points**:
- **In a Data Flow diagram, writing `ULyraHealthComponent` bare is enough**
  (plain-text CamelCase extraction) — backticks are not mandatory, but adding
  them is safer.
- **The Evidence `path:line` is checked against the code by the engine**:
  match → the evidence is trustworthy; mismatch → a `⚠ drift` warning (the
  code moved and the doc is stale). This is the lever that keeps documents
  fresh.

---

## 4. The kind keyword table (title word-choice reference)

`classify_kind` matches **keyword substrings** in titles (case-insensitive).
To get a kind, the title must contain its word:

| Desired kind | Title must contain | Example |
|--------------|--------------------|---------|
| data_flow | `flow` / `data flow` | `## Data Flow` |
| architecture | `architect` | `## Architecture` |
| responsibility | `responsib` | `### Class Responsibilities` |
| data_structure | `struct` | `### Key Structs` |
| design_decision | `claim` | `## Key Claims` |
| boundary | `boundar` | `## Boundaries` |
| dependency | `depend` | `## Dependencies` |
| evidence | `evidence` | `## Evidence` |
| context | `context` | `## Context` |
| impact | `risk` / `impact` | `## Impact & Risks` |
| edge_case | `edge case` | `## Edge Cases` |
| (anything else) | — | falls back to generic `section` |

> Conversely: a **casually named** title silently becomes the semantics-less
> `section` kind and cannot be precisely ranked by kind at retrieval time.
> **Follow the standard section names.**

---

## 5. How to change: the maintenance loop

### 5.1 Recompile after every change
```
brain-rs --project-root <project root> compile    # project knowledge → project brain
brain-rs compile --pack packs/<name>              # pack knowledge → the pack's own db
```
- Document compilation is a **full rebuild** (`DELETE FROM nodes`, then
  re-split), not incremental — but at this scale it finishes in about a
  second, so don't worry.
- Code scanning (`scan`) **is** incremental; document changes only need
  `compile`, never a re-`scan`.

### 5.2 The self-check commands (the quality feedback loop)
After changing documents, verify **with the engine**, not by feel:

0. **Lint first** (pre-compile, hard gate) — document format, directory
   layout, and pack references:
   ```
   brain-rs lint            # errors exit non-zero; run before every compile
   brain-rs lint --pack packs/<name>
   ```
   Named rules: `frontmatter-missing/no-tier`, `tier-conflict`,
   `feature-needs-module`, `heading-indent/no-space`, `claims-not-bulleted`,
   `evidence-malformed`, `tier-dir-mismatch` (errors); `section-kind-generic`,
   `missing-architecture`, `root-doc-misplaced`, `nested-docs-dir`,
   `pack-index-stale`, `schema-missing-section` (warnings); `pack-not-found`,
   `pack-index-missing` (reference errors). Fix all errors before compiling.

1. **Check the gate** — did you write sections that will be
   quarantined/degraded:
   ```
   brain-rs contract
   ```
   The output lists every `empty-leaf` (empty section) / `thin-content` (too
   short) / `missing-envelope` violation with reason and line number. Goal:
   newly written sections do not appear in it.

2. **Check drift** — do Evidence `path:line` locations still match the code
   (run after `compile`):
   ```
   brain-rs refs <a symbol you cite>
   ```
   Seeing `⚠ drift: code index resolved <another file>` → the code moved;
   update the Evidence.

2.5 **Heed feedback warnings** — if a packet warns `agent feedback … marked
   'wrong'/'stale'`, that document has failed a real user: prioritise fixing
   it, recompile, then `brain-rs feedback --clear <node_id>`.

---

## 6. The tier schema (required sections, enforced as warnings)

Every document tier carries a **schema**: the set of section kinds it is
expected to contain. The schema is checked twice — by `brain-rs lint`
(source time) and by `brain-rs compile` (persisted as warning-level
violations, shown in the post-compile health report and `brain-rs contract`)
— so gaps surface where authors and agents look, and get fixed deliberately,
never auto-rewritten.

**Matching rules:**
- Sections match by **kind keyword**, never by exact title: `## Heat Flow`
  satisfies `data_flow`, `### Class Responsibilities` satisfies
  `responsibility`. Renaming a section never breaks conformance.
- A kind is satisfied by a section at **any heading depth** — `## Context`
  or a nested `### Context` both count. Grow sub-sections freely.
- Missing sections are **warnings**, not errors.

**Built-in required kinds per tier:**

| Tier | Required section kinds |
|------|------------------------|
| `architecture` (root L0) | context, architecture, data_flow, design_decision, boundary, evidence |
| `domain` | context, architecture, data_flow, design_decision, boundary, evidence |
| `module` | context, architecture, data_flow, design_decision, edge_case, boundary, evidence |
| `feature` | context, architecture, data_flow, design_decision, edge_case, boundary, evidence |

**Overriding per project** (`brain.toml`) — a tier listed here fully replaces
the built-in list for that tier:

```toml
[schema]
feature = ["context", "boundary", "evidence"]
```

**Evolution discipline** (how the schema may change without document churn):
1. The kind keyword table is **additive-only** — adding keywords can only
   make more titles conform, never fewer.
2. New requirements enter as warnings; nothing becomes a hard error without
   a migration window.
3. Mechanical fixes belong in tooling, not in by-hand edits across docs.

3. **Check answerability** — can your document actually answer its target
   question:
   ```
   brain-rs query "<the question this doc should answer>"
   ```
   Read the hit unit's `answerability`: `sufficient` is the pass bar;
   `partial`/`insufficient` means weak evidence or hollow content — add
   claims/Evidence.

### 5.3 Change checklist (AI: walk every item when maintaining)
- [ ] New knowledge bases/packs were scaffolded with `brain-rs init` /
  `brain-rs init --pack`; the directory structure was not hand-created
- [ ] New module docs started from `brain-rs scaffold <code dir>` where a
  code index exists (evidence locations are then real by construction)
- [ ] Frontmatter's first line is `---`; exactly one tier field:
  `architecture:`/`domain:`/`module:`; feature docs use `feature:` + `module:`
- [ ] Standard sections (Context/Architecture/Claims/Boundaries/Evidence) use
  the §4 keywords; custom body-section titles are the allowed exception
- [ ] Every new section body has ≥ 30 substantive characters (else degraded)
- [ ] Key Claims / Boundaries: every assertion is its own bullet, names its subject (no bare "It…"), and backticked symbols resolve
- [ ] Domain/module/feature docs have a `## Boundaries` section stating what they do **not** cover
- [ ] Evidence format is strict: `` `symbol` defined at `path:line` ``
- [ ] Ran `lint` — zero errors (warnings explained or fixed)
- [ ] Ran `compile` + `contract` — no new violations
- [ ] Ran `refs` — no unexpected drift
- [ ] Ran `query` on the target question — reached `sufficient`

---

## 6. Anti-patterns (do not write like this)

| Anti-pattern | Consequence | Correct approach |
|--------------|-------------|------------------|
| Indented headings `  ## X` or Setext (`===`) | Not recognised as a heading; the section merges into the previous one | Flush-left ATX `## X` |
| `#title` (no space after `#`) | Not a heading | `# Title` (with a space) |
| One-sentence sections / empty placeholder sections | Degraded by `thin-content` / quarantined by `empty-leaf` | Write ≥ 30 chars or delete the section |
| Splitting fragments **not worth querying alone** (a short note) into standalone files | Loses the Context Envelope; retrieval fragments | Keep them as `###` subsections of the module doc (§1.4) |
| Cramming deep topics **worth querying alone** (algorithms) into the module doc | Bloats and defocuses the module doc; forced recompiles on every topic change | Standalone feature doc (§1.3) |
| Evidence written as prose, "defined in file XX" | Symbol/line cannot be parsed; no anchor | Strict: `` `symbol` defined at `path:line` `` |
| Assertions written as one big paragraph | Cannot be extracted as individual claims | Split into `- ` bullets |
| Complex YAML in frontmatter (nesting/arrays as structure) | Only simple `key: value` is read; complex structure silently ignored | Keep single-line `key: value` |
| Non-knowledge files (README/specs) placed inside a knowledge root | Indexed as knowledge nodes, polluting retrieval | Keep them outside the knowledge root |

---

## 7. Known limitations (honestly documented)

- **No incremental document compilation**: changing one document rebuilds all
  nodes (finishes in about a second at current scale — acceptable).
- **Lint is a CLI gate, not yet editor-integrated**: `brain-rs lint` catches
  format/layout/reference violations pre-compile (and exits non-zero for CI
  or pre-commit), but there is no in-editor "error while writing" feedback.
- **kind relies on keyword substrings**: a poorly chosen title silently falls
  back to the generic `section` kind with no error — following §4 strictly is
  the only guarantee.
- **No ignore mechanism inside knowledge roots**: every `.md` is indexed;
  "put files in the right place" is the control, not configuration.
