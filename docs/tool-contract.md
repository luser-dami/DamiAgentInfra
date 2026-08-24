# The DamiAgentInfra tool contract

This document is the toolbox contract. It is a **convention**, not a library:
every module implements it independently in its own codebase (~50 lines of
boilerplate). There is deliberately no shared contract package, so modules
stay copyable and publishable on their own.

## Invocation shape

Every tool exposes the same command shape:

```
<tool> <verb> [args] [--json]
```

- `<tool>` is the executable name (e.g. `alexandria`, `dami-docs`).
- `<verb>` is a lowercase action word (`scan`, `query`, `put`, `gen`, …).
- `--json` selects machine-readable output.

## Output channels

- **Machine-readable output is JSON on stdout** when `--json` is passed.
  Stdout carries data only — no progress, no banners, no warnings.
- **Human output goes to stderr**: progress, warnings, summaries, and the
  pretty-printed form of results when `--json` is absent.

This split lets agents pipe stdout into `jq` or another tool without
filtering noise.

## Exit codes

| Code | Meaning                                                        |
| ---- | -------------------------------------------------------------- |
| 0    | Success                                                        |
| 2    | Usage / argument error (bad verb, missing or invalid argument) |
| 3    | Environment error (missing dependency, runtime, or toolchain)  |
| 4    | Domain error (e.g. project not indexed, store not initialized) |

Code 1 is reserved for unexpected internal failures (panics, bugs).

## Self-description: `--describe`

Every tool implements:

```
<tool> --describe
```

It prints a JSON self-description to stdout containing:

- `name`, `version`, and a one-line `summary`;
- `contract`: the contract version the tool implements (currently `1`);
- `verbs`: each verb with its argument schema expressed as JSON Schema.

`--describe` output is the single source of truth that feeds future MCP
server generators and skill generators — they consume this JSON instead of
parsing help text.

## Versioning

This is **contract version 1**. Modules state the contract version they
implement in their `--describe` output. Breaking changes to this document
increment the contract version; modules opt into newer versions individually,
in line with the toolbox's per-module release model (`<module>-vX.Y.Z`).
