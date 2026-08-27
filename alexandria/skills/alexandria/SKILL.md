---
name: alexandria
description: Query and maintain the project's Alexandria knowledge library (.alexandria/) — code index + authored knowledge + lessons. Use BEFORE editing code (what is this symbol, who calls it, what knowledge covers it), when the user reports a bug (recall lessons first), and AFTER resolving a non-obvious error (record a lesson). Triggers: 查代码/定位符号/影响面/以前踩过这个坑吗/记录一下/这文档还新吗.
---

# Alexandria — project knowledge engine

Alexandria compiles a project's source code and hand-written Markdown knowledge
into a single SQLite library (`.alexandria/index/alexandria.db`), then answers
queries with a self-contained **Evidence Packet**: the relevant knowledge, its
ancestry, the code symbols it cites (resolved to real `file:line`), claim
verification grades, and a self-assessed answerability. Use it to get a
trustworthy, cited answer in **one round-trip** instead of grepping the repo
across many.

## When to use (and when not)

- **Before editing code** → `query` the subsystem; past lessons and documented
  boundaries override your plan. A lesson hit is authoritative.
- **User reports a bug / "不生效/还是不行"** → FIRST action is `query` with the
  symptom + subsystem name, before reading code or proposing causes. When the
  task context is known (building, editor running, CI…), declare it:
  `query "..." --context ubt-build,editor-running` — lesson applicability is
  then matched exactly; without it the packet only discloses each lesson's
  applies-when/excludes and you judge the context yourself.
- **Impact analysis** → `graph callers|callees <symbol>`, `refs <symbol>`.
- **After resolving a non-obvious error** → write a lesson (see below), then
  `compile`.
- **Not for**: reading file contents (use the file directly), editing code,
  build/test execution.

## Setup (per project, once)

```bash
alexandria init --project-root .        # scaffolds .alexandria/{alexandria.toml, knowledge/}
# edit .alexandria/alexandria.toml: [scan] include_dirs = ["Source", "Plugins"]
alexandria scan                          # code index (incremental; re-run after code changes)
alexandria compile                       # knowledge compile (re-run after doc changes)
alexandria status                        # sanity: symbols/edges/nodes counts
```

## Daily commands

```bash
alexandria query "how does the melee combo window work"   # Evidence Packet(s)
alexandria query --format json "..." [--limit 5]          # machine-readable
alexandria locate USkillFragment                          # symbol -> file:line + signature
alexandria refs USkillFragment                            # which knowledge cites it
alexandria graph callers UWeapon::Fire                    # impact: who calls it
alexandria graph dependents SomeHeader.h                  # impact: who includes it
alexandria lint                                           # knowledge-base format gate
alexandria contract                                       # Chunk Contract audit
alexandria feedback useful --query "..." [--node <id>]     # verdicts: useful|partial|wrong|stale
```

Output: `--format text` (humans) / `json` (machines) / `tagged` (LLM-tuned).

## Reading an Evidence Packet

- `status: accepted` + `verification: verified` → trust and proceed.
- `answerability.level` is the engine's own confidence; `recommended_action`
  tells you whether to proceed, verify further, or escalate to the user.
- `warnings` (drift, stale bindings, prior negative feedback) → treat the
  content as suspect until checked against source.

## Authoring knowledge

Docs live in `.alexandria/knowledge/` under tiers: `Architecture.md`,
`domains/`, `modules/`, `features/`, `lessons/`. Frontmatter declares the tier
(`module: Game/Weapon`, `domain: Combat`, `feature: WeaponHeat`, `lesson: <id>`).
Standard sections: `## Key Claims` (`- [extracted]` = machine-checkable fact,
`- [inferred]` = design belief), `## Boundaries`, `## Evidence` (cite
`` `Symbol` defined at `path` `` — no line number; verification is
file-level and displayed locations come from the live code index). Claims
marked `[extracted]` are verified
against the code index at compile/query time — drift is flagged, not hidden.

## Writing a lesson (after a resolved error)

Create `.alexandria/knowledge/lessons/<slug>.md` with frontmatter
`lesson: <slug>` and exactly these sections:

Optional applicability frontmatter (declare what you know — all exact-match
slugs, never guessed by the engine):

- `applies-when: [slug, …]` — contexts where the lesson holds (e.g.
  `ubt-build`, `editor-running`).
- `excludes: [slug, …]` — contexts where it explicitly does NOT apply.
- `guard-strength: directive|scope|hint|reference` — how much judgement the
  Guard pre-empts (default `hint`). Use `directive` only for deterministic
  checks ("run X before Y"), never for judgement calls.
- `depends-on: <prose>` — what the conclusion rests on; re-verify the lesson
  when that changes.

- `## Symptom` — verbatim error/observation (commands, output blocks).
- `## Root Cause` — the actual mechanism, not the guess that cost time.
- `## Fix` — what resolved it, step by step.
- `## Guard` — the rule to apply NEXT TIME before forming hypotheses.
- `## Evidence` — file:line references backing the root cause.

Then `alexandria compile`. A lesson that can't name a Guard is not finished.

## Working rules

- Query first, grep second. An Evidence Packet that answers = skip the repo dive.
- Retrieval scoring is **fully automatic**: every query is passively captured,
  verdicts are inferred from mechanical signals, and `compile` replays the
  eval dataset. Never call `eval`/`feedback` manually — they are not agent
  workflows.
- After code changes: `scan`. After doc edits: `lint && compile`.
- When a query answer proves wrong/outdated in conversation, record it:
  `feedback wrong --query "..." --node <node_id>` — later packets
  carry the warning until the doc is fixed and the record cleared.
- When the user confirms a lesson's Guard fixed the failure, record the
  efficacy: `feedback applied-resolved --query "..." --node <lesson node>`.
  (applied-failed is auto-recorded by the harness on same-symptom
  recurrence; 2+ consecutive failures demote the lesson, 3+ resolves flag
  it in `status` as a graduation candidate.)
- Never edit `.alexandria/index/` by hand; regenerate with scan/compile.
