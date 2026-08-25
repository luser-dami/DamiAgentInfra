---
name: dami-harness
description: Manage AI-agent harness resources (skills/rules/docs/agents/hooks/MCP) in a local store and install them into agent tool directories (.claude/, .codex/, .cursor/, .omp/). Use when adding or removing a skill for your agents, injecting hooks or MCP servers into agent settings, checking harness health, or setting up a new machine. Triggers: 装个 skill/同步技能/hooks 注入/MCP 配置/新机器初始化.
---

# dami-harness — local harness resource manager

dami-harness keeps a versioned local store of agent resources
(`~/.dami-harness/store/`) and reconciles them into detected AI coding agents.
Personal and standalone: no git remotes, no teams.

## Concepts

- **Store**: `~/.dami-harness/store/<type>/` where type is
  `skills|rules|docs|agents|hooks|mcp`. You own these files; edit them directly.
- **Agents**: target tools — `claude`, `codex`, `cursor`, `omp`, plus mainstream
  skill-directory agents (gemini, aider, copilot, …). Detection = the tool's
  directory exists (e.g. `~/.omp/`); agents explicitly named in `init --agent`
  get their directory created.
- **Reconcile**: hooks and MCP servers are injected idempotently into each
  tool's settings (managed-manifest pattern — re-runs are safe, stale entries
  are cleaned up).

## Setup

```bash
dami-harness init --scope user --agent claude,omp   # create store + config, inject hooks
dami-harness doctor                                  # health check (config, store, hooks)
dami-harness status                                  # store status + resource counts
```

Project scope (`--scope project`, the default inside a project) stores config
under `<project>/.dami-harness/` instead of `~/`.

## Adding a skill (the everyday task)

```bash
mkdir -p ~/.dami-harness/store/skills/my-skill
$EDITOR ~/.dami-harness/store/skills/my-skill/SKILL.md   # frontmatter: name, description
dami-harness init --force --agent claude,omp             # idempotent: installs store skills
dami-harness list skills                                 # verify: store + installed locations
```

The skill lands at `~/.claude/skills/my-skill/SKILL.md`,
`~/.omp/skills/my-skill/SKILL.md`, etc. `dami-harness skill show my-skill`
shows metadata; `dami-harness skill exclude my-skill` stops installing it
without deleting it from the store.

## Hooks and MCP

```bash
dami-harness hooks list                    # what would be injected
dami-harness hooks inject                  # write hook entries into tool settings
dami-harness hooks remove                  # strip managed entries
dami-harness mcp list|inject|remove        # same for MCP server definitions
```

Hooks run through the hidden `hook-dispatch` command, which the injected
settings call on events (session-start, stop, post-tool-use, prompt-submit).

## Machine use

Every command supports `--json` (JSON on stdout, human output on stderr).
Exit codes: 0 ok · 2 usage · 3 environment · 4 domain (e.g. not initialized).
`dami-harness --describe` prints the JSON self-description of all verbs.

## Working rules

- Edit resources in the **store**, never in the agent directories — reconcile
  overwrites managed content there.
- `init` is idempotent and additive: re-running it is the way to sync.
- `dami-harness uninstall` removes injected content and (with flags) the
  store — check `uninstall --help` first.
- If an agent you named is not receiving content, run `doctor` — the most
  common cause is the tool directory not existing (init now creates it for
  explicitly named agents).
