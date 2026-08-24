import { z } from 'zod';
import path from 'node:path';

// ─── Tool path config ───────────────────────────────────

export const ToolPathsSchema = z.object({
  skills: z.string().optional(),
  rules: z.string().optional(),
  settings: z.string().optional(),
  claudemd: z.string().optional(),
  /** Per-tool agents directory. Optional — tools without subagent support omit
   * this and agents sync skips them. */
  agents: z.string().optional(),
  /** User-scope MCP config file (relative to $HOME). Omitted = tool has no MCP support. */
  mcp: z.string().optional(),
  /** Project-scope MCP config file. Never defaults from `mcp` — omitting it means
   * the tool has no project-scope MCP support at all. Claude Code shows why the two
   * cannot share a value: user scope is ~/.claude.json but project scope is
   * <root>/.mcp.json, breaking the usual `.<tool>/<file>` convention. */
  mcpProject: z.string().optional(),
});

// ─── Scope ──────────────────────────────────────────────

export const ScopeEnum = z.enum(['user', 'project']);
export type Scope = z.infer<typeof ScopeEnum>;

// ─── Harness store manifest (dami-harness.yaml) ──────────

export const SharingConfigSchema = z.object({
  skills: z.object({}).default({}),
  rules: z.object({
    enforced: z.array(z.string()).default([]),
  }).default({}),
  docs: z.object({
    localDir: z.string().default('~/.dami-harness/docs'),
  }).default({}),
  // Optional (not .default) so existing HarnessConfig literals stay valid; use
  // getHooksSharing() for the defaulted view.
  hooks: z.object({
    /** Auto-apply store hooks during resource sync. When false, sync only hints;
     *  the user must run `hooks inject` to apply (explicit consent). */
    autoApply: z.boolean().default(true),
    /** Restrict store hook commands to scripts under the harness home team-scripts dir. */
    requireTeamScripts: z.boolean().default(false),
  }).optional(),
  // Optional (not .default) so existing HarnessConfig literals stay valid; use
  // getMcpSharing() for the defaulted view.
  mcp: z.object({
    /** Auto-apply store MCP servers during resource sync. When false, sync only
     *  hints; the user must run `mcp inject` to apply (explicit consent). */
    autoApply: z.boolean().default(true),
    /** Allowed stdio commands. Empty = no restriction. */
    allowedCommands: z.array(z.string()).default([]),
    /** Allowed http/sse hosts (supports a leading `*.` wildcard). Empty = no restriction. */
    allowedHosts: z.array(z.string()).default([]),
  }).optional(),
});

/** Defaulted view of the optional `sharing.hooks` config. */
export function getHooksSharing(config: { sharing?: { hooks?: { autoApply?: boolean; requireTeamScripts?: boolean } } }): {
  autoApply: boolean;
  requireTeamScripts: boolean;
} {
  const h = config.sharing?.hooks;
  return {
    autoApply: h?.autoApply ?? true,
    requireTeamScripts: h?.requireTeamScripts ?? false,
  };
}

/** Defaulted view of the optional `sharing.mcp` config. */
export function getMcpSharing(config: {
  sharing?: { mcp?: { autoApply?: boolean; allowedCommands?: string[]; allowedHosts?: string[] } };
}): { autoApply: boolean; allowedCommands: string[]; allowedHosts: string[] } {
  const m = config.sharing?.mcp;
  return {
    autoApply: m?.autoApply ?? true,
    allowedCommands: m?.allowedCommands ?? [],
    allowedHosts: m?.allowedHosts ?? [],
  };
}

export const HarnessConfigSchema = z.object({
  team: z.string(),
  description: z.string().default(''),
  /** Ignored by `init` — local install scope is decided only by CLI `--scope` /
   * default. Kept optional so old manifest files still parse. */
  scope: ScopeEnum.optional(),
  sharing: SharingConfigSchema.default({}),
  // MCP paths are only set for tools whose config location has been verified.
  // Tools left without `mcp` are skipped by MCP sync rather than guessed at, so a
  // wrong guess can never create a junk config file on a user's machine.
  toolPaths: z.record(z.string(), ToolPathsSchema).default({
    claude: { skills: '.claude/skills', rules: '.claude/rules', settings: '.claude/settings.json', claudemd: '.claude/CLAUDE.md', agents: '.claude/agents', mcp: '.claude.json', mcpProject: '.mcp.json' },
    codex: { skills: '.codex/skills', rules: '.codex/rules', settings: '.codex/hooks.json', agents: '.codex/agents', mcp: '.codex/config.toml' },
    cursor: { skills: '.cursor/skills', rules: '.cursor/rules', settings: '.cursor/hooks.json', agents: '.cursor/agents', mcp: '.cursor/mcp.json', mcpProject: '.cursor/mcp.json' },
    // OMP (Oh My Pi): .omp/skills/<name>/SKILL.md maps 1:1 onto the skills
    // channel. AGENTS.md is the project-manual analog of CLAUDE.md (legacy
    // section cleanup target only). No rules directory, no settings.json hook
    // file (OMP hooks are executable .ts under .omp/hooks/), and no observed
    // MCP config — those channels are left unsupported.
    omp: { skills: '.omp/skills', claudemd: '.omp/AGENTS.md' },
  }),
});

export type HarnessConfig = z.infer<typeof HarnessConfigSchema>;

// ─── Local config (config.yaml under the harness home) ─────

export const LocalConfigSchema = z.object({
  repo: z.object({
    /** Absolute path of the local harness store directory. */
    localPath: z.string(),
    /** Legacy field, unused by the local harness. Kept for config compat. */
    remote: z.string().default(''),
    /** Legacy field; the local harness never uses 'http'/'self' modes. */
    kind: z.enum(['git', 'http', 'self']).optional(),
  }),
  username: z.string(),
  // Read-compat default for historical configs that omit `scope` (pre-project era).
  scope: ScopeEnum.default('user'),
  /** Absolute path to project root; required when scope is 'project'. */
  projectRoot: z.string().optional(),
  /** Opt-in: include safe user-scope resources while in project scope. */
  inheritUserScope: z.boolean().optional(),
  /** Tags the user has subscribed to. If empty/undefined, sync all resources. */
  subscribedTags: z.array(z.string()).optional(),
  /** Skills to exclude from local sync (per-user, does not affect the store). */
  excludedSkills: z.array(z.string()).optional(),
  /** When set, only inject hooks into these agents. Additive across multiple init --agent runs. */
  enabledAgents: z.array(z.string()).optional(),
  /** Tools explicitly excluded from all harness sync (set by `uninstall --agent`). Removed again by `init --agent`. */
  disabledAgents: z.array(z.string()).optional(),
});

export type LocalConfig = z.infer<typeof LocalConfigSchema>;
export type LocalConfigInput = z.input<typeof LocalConfigSchema>;

// ─── Local state (state.json under the harness home) ───────

export const StateSchema = z.object({
  lastPush: z.string().nullable().default(null),
  lastPull: z.string().nullable().default(null),
  /** Git commit hash (short) of the store at the time of last successful sync. */
  lastPullRev: z.string().nullable().default(null),
  /** Git commit hash synchronized through the safe user-resource inheritance channel. */
  lastInheritedPullRev: z.string().nullable().optional(),
  pushedRules: z.array(z.string()).default([]),
  pushedSkills: z.array(z.string()).default([]),
  pushedEnvVars: z.array(z.string()).default([]),
});

export type State = z.infer<typeof StateSchema>;

// ─── Tags config (store: tags.yaml) ─────────────────────
//
//  Centralized tag-to-resource mapping.
//  Users subscribe to tags in their local config; sync filters resources
//  by matching tags.
//
//  Backward compat rules:
//    - No tags.yaml → sync everything
//    - No subscribedTags → sync everything
//    - Resource not in tags.yaml → always synced (untagged = universal)
//

/** Parsed content of <store>/tags.yaml. */
export interface TagsConfig {
  /** Skill name → list of tags. */
  skills: Record<string, string[]>;
  /** Rule name → list of tags. */
  rules: Record<string, string[]>;
}

// ─── Resource types ─────────────────────────────────────

export type ResourceType = 'skills' | 'rules' | 'docs' | 'agents' | 'hooks' | 'mcp';

export type ResourceItemStatus = 'new' | 'modified';

export interface ResourceItem {
  name: string;
  type: ResourceType;
  sourcePath: string;
  relativePath: string;
  status?: ResourceItemStatus;
  namespace?: string;
}

export interface ResourceDiff {
  added: ResourceItem[];
  modified: ResourceItem[];
  removed: ResourceItem[];
}

// ─── Hook definitions (unified model) ────────────────────
//
//  A single declarative model for both built-in operational hooks (source:
//  'builtin', shipped with the CLI) and store-defined hooks (source: 'team',
//  declared in the store's hooks/hooks.yaml). One `reconcileHooks()` engine
//  injects both.
//
//  `event` is always the Claude PascalCase name (the cross-tool lingua
//  franca); the engine maps it to Cursor's camelCase via CLAUDE_TO_CURSOR_EVENTS.

export interface HookDef {
  /** Distinguishes CLI built-in (A) from store-declared (B) hooks. */
  source: 'builtin' | 'team';
  /** Stable identity: builtin = description keyword, team = yaml `id`. */
  key: string;
  /** Claude PascalCase event name (SessionStart/Stop/PostToolUse/UserPromptSubmit). */
  event: string;
  /** Optional tool matcher (e.g. "Bash", "Skill"). "*" or undefined = all. */
  matcher?: string;
  /** Shell command to run. */
  command: string;
  /** Per-hook timeout in seconds (tool-specific; omitted = tool default). */
  timeout?: number;
  /** settings.json description. builtin: "[dami-harness] <key>"; team: "[dami-harness:hook:<id>] ...". */
  description: string;
  /** Store hooks only: restrict to these tools (default = all hook-capable tools). */
  tools?: string[];
}

// ─── MCP server definitions ──────────────────────────────
//
//  Store-declared MCP servers (mcp/mcp.yaml) are parsed into this tool-neutral
//  model, then rendered per tool by resources/mcp-format.ts — the same
//  "intermediate model → per-tool render" shape agents already uses.
//
//  Ownership is tracked out-of-band in managed-mcp.json under the harness home,
//  because an MCP entry has no free-text field to stamp a marker into (hooks
//  stamp `[dami-harness:hook:<id>]` into `description`). This mirrors how hooks
//  already track Cursor/Codex entries, which have no description either.

export type McpTransport = 'stdio' | 'http' | 'sse';

export interface McpServerDef {
  /** Server key as written into each tool's config. */
  name: string;
  description?: string;
  transport: McpTransport;
  /** stdio only. */
  command?: string;
  args?: string[];
  /** http/sse only. */
  url?: string;
  /** http/sse only. Values may contain ${VAR} placeholders. */
  headers?: Record<string, string>;
  /** Env vars passed to the server process. Values may contain ${VAR} placeholders. */
  env?: Record<string, string>;
  /** Request timeout in milliseconds, passed through where the tool supports it. */
  timeout?: number;
  /** Executables that must be on PATH; missing ones cause a skip-with-hint. */
  requires?: string[];
  /** Restrict to these tools (default = every MCP-capable tool). */
  tools?: string[];
}

/** One injected MCP server recorded in the manifest. */
export interface ManagedMcpRecord {
  name: string;
  /** sha1 (first 16 hex) of the rendered entry; drives idempotent rewrites. */
  hash: string;
}

/** managed-mcp.json — store MCP servers injected per tool+scope key. */
export type ManagedMcpManifest = Record<string, ManagedMcpRecord[]>;

/** Path of the managed-MCP manifest for a scope. */
export function managedMcpManifestPath(scope: Scope, projectRoot?: string): string {
  return path.join(getDamiHome(scope, projectRoot), 'managed-mcp.json');
}

// ─── Global options ─────────────────────────────────────

export interface GlobalOptions {
  dryRun?: boolean;
  verbose?: boolean;
  silent?: boolean;
  /** Machine-readable JSON output on stdout (tool contract). */
  json?: boolean;
  /** Force full sync even when store state looks up to date. */
  force?: boolean;
  /** Operate on a specific skill by path. */
  skill?: string;
}

// ─── Constants ──────────────────────────────────────────

export const DAMI_HOME = `${process.env.HOME}/.dami-harness`;
export const DAMI_CONFIG_PATH = `${DAMI_HOME}/config.yaml`;
export const DAMI_STATE_PATH = `${DAMI_HOME}/state.json`;
export const RESOURCE_TYPES: ResourceType[] = ['skills', 'rules', 'docs', 'agents', 'hooks', 'mcp'];

export const DAMI_RULES_START = '<!-- [dami-harness:rules:start] -->';
export const DAMI_RULES_END = '<!-- [dami-harness:rules:end] -->';

export const DAMI_HOOK_DESCRIPTION_PREFIX = '[dami-harness]';

/**
 * Description prefix for store-declared (B) hooks. Deliberately NOT starting with
 * a bare "[dami-harness]" token boundary so the two marker namespaces never collide:
 * built-in detection matches "[dami-harness] " / command markers, team detection
 * matches "[dami-harness:hook:". Format: "[dami-harness:hook:<id>] <description>".
 */
export const DAMI_CUSTOM_HOOK_PREFIX = '[dami-harness:hook:';

/**
 * Description prefix for externally managed agent hooks installed via an
 * `install_hook_rule`-style sync command. A third, isolated marker namespace:
 * it does NOT start with "[dami-harness] " (built-in) nor "[dami-harness:hook:" (team), so
 * a full reconcile treats agent hooks as untouched and never deletes them.
 * Format: "[dami-harness:agent-hook:<slug>]".
 */
export const DAMI_AGENT_HOOK_PREFIX = '[dami-harness:agent-hook:';

// ─── Scope helpers ─────────────────────────────────────

/**
 * Resolve the base directory for resource installation based on scope.
 * - user scope  → process.env.HOME (e.g. /Users/xxx)
 * - project scope → localConfig.projectRoot (e.g. /Users/xxx/my-project)
 */
export function resolveBaseDir(localConfig: LocalConfig): string {
  if (localConfig.scope === 'project') {
    if (!localConfig.projectRoot) {
      throw new Error(
        'resolveBaseDir: localConfig.scope is "project" but projectRoot is missing — ' +
        'refusing to silently fall back to the user home directory. Re-run `init` in this project.',
      );
    }
    return localConfig.projectRoot;
  }
  return process.env.HOME!;
}

/** True when `tool` is in localConfig.disabledAgents (excluded from harness sync). */
export function isAgentDisabled(localConfig: { disabledAgents?: string[] }, tool: string): boolean {
  return localConfig.disabledAgents?.includes(tool) ?? false;
}

/**
 * Single-repo mode is a team-workflow feature and does not exist in the local
 * harness. This stub always returns false so legacy self-mode code paths in the
 * resource handlers stay dormant.
 */
export function isSelfMode(_localConfig: { repo: { kind?: string } }): boolean {
  return false;
}

/**
 * Get the harness home directory for a given scope.
 * - user scope  → ~/.dami-harness (evaluated at call time for test compatibility)
 * - project scope → <projectRoot>/.dami-harness
 */
export function getDamiHome(scope: Scope, projectRoot?: string): string {
  if (scope === 'project') {
    if (!projectRoot) {
      throw new Error(
        'getDamiHome: scope is "project" but projectRoot is missing — ' +
        'refusing to silently fall back to the user home directory.',
      );
    }
    return path.join(projectRoot, '.dami-harness');
  }
  return path.join(process.env.HOME ?? '', '.dami-harness');
}

/**
 * Get the config.yaml path for a given scope.
 */
export function getConfigPath(scope: Scope, projectRoot?: string): string {
  return path.join(getDamiHome(scope, projectRoot), 'config.yaml');
}

/**
 * Get the state.json path for a given scope.
 */
export function getStatePath(scope: Scope, projectRoot?: string): string {
  return path.join(getDamiHome(scope, projectRoot), 'state.json');
}

/**
 * Get the managed-hooks manifest path for a given scope. This file indexes the
 * store (B) hooks injected into each tool, so reconcile can clean up hooks that
 * were removed from hooks.yaml (esp. for Cursor, whose entries carry no marker).
 */
export function getManagedHooksPath(scope: Scope, projectRoot?: string): string {
  return path.join(getDamiHome(scope, projectRoot), 'managed-hooks.json');
}

/**
 * Get the user-level pushignore path.
 */
export function getPushignorePath(): string {
  return path.join(process.env.HOME ?? '', '.dami-harness', 'pushignore');
}

/**
 * Local kill-switch for store (B) hooks. Set DAMI_HOOKS_DISABLED=1 to veto
 * store-declared hooks on this machine (built-in operational hooks still apply).
 */
export function areTeamHooksDisabled(): boolean {
  return process.env.DAMI_HOOKS_DISABLED === '1' || process.env.DAMI_HOOKS_DISABLED === 'true';
}
