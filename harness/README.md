# dami-harness

A local harness resource manager for AI coding agents.

dami-harness keeps a versioned **local store** of agent resources —
`skills/`, `rules/`, `docs/`, `agents/`, `hooks/`, `mcp/` — and scans
or injects them into the directories of detected agent tools (`~/.claude`,
`~/.codex`, `~/.cursor`, OMP's `.omp/`, plus other mainstream agents via the
known-agents registry) via per-type `ResourceHandler` classes. Hooks and MCP
servers are reconciled idempotently into each tool's settings through a
managed-manifest pattern, and a unified `hook-dispatch` entry point runs
inside host-IDE hook subprocesses.

Everything is local: no git remotes, no HTTP daemon, no analytics.

## Install

```sh
npm install
npm run build
# then link or run directly:
node dist/cli.js --help
```

## CLI surface

```
dami-harness init                 # create the local store, write config, inject hooks
dami-harness status               # store status, sync state, resource counts
dami-harness list [type]          # skills|rules|docs|agents|hooks|mcp
dami-harness skill show <name>    # skill metadata
dami-harness skill exclude ...    # per-user skill exclusion (list/add/remove)
dami-harness hooks list|inject|remove
dami-harness mcp   list|inject|remove
dami-harness doctor               # diagnose configuration
dami-harness uninstall            # remove all managed resources and hooks
dami-harness hook-dispatch        # (hidden) unified IDE-hook dispatcher
```

Supported agent tools for hook/MCP reconciliation: **claude**, **codex**,
**cursor**, **omp**. State lives in `~/.dami-harness` (user scope) or
`<project>/.dami-harness` (project scope); the resource store is
`<home>/store` with a `dami-harness.yaml` manifest.

## Tool contract

dami-harness implements the DamiAgentInfra tool contract: `--json` emits
machine-readable JSON on stdout with human output on stderr, exit codes are
0/2/3/4, and `dami-harness --describe` prints a JSON self-description of every
verb. See [../docs/tool-contract.md](../docs/tool-contract.md).

## Provenance

Derived from Tencent's [teamai-cli](https://github.com/Tencent/teamai-cli)
(MIT) — the team/sync features were removed and the remainder was rebranded.
See [LICENSE](LICENSE).

## Development

```sh
npm run typecheck   # tsc --noEmit
npm test            # vitest run
npm run build       # tsup → dist/cli.js
```
