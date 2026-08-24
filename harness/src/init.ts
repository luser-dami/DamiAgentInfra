import YAML from 'yaml';
import fs from 'node:fs';
import path from 'node:path';
import { saveLocalConfig, loadTeamConfig, saveLocalConfigForScope, loadLocalConfigForScope, loadStateForScope, saveStateForScope } from './config.js';
import { reconcileTeamHooksForConfig } from './hooks.js';
import { ensureDir, writeFile, pathExists, expandHome } from './utils/fs.js';
import { log } from './utils/logger.js';
import { DAMI_HOME, type GlobalOptions, type LocalConfig, type Scope, getDamiHome, getConfigPath, resolveBaseDir } from './types.js';
import { askConfirmation, closePrompt } from './utils/prompt.js';
import { normalizeAgentList } from './known-agents.js';
import { deployBuiltinSkills } from './builtin-skills.js';
import { deployBuiltinRules } from './builtin-rules.js';
import { deployBuiltinAgents } from './builtin-agents.js';
import { SkillsHandler } from './resources/skills.js';

/** Resolve + realpath so macOS /var → /private/var (and similar) compare equal. */
function resolveRealPath(p: string): string {
  const resolved = path.resolve(p);
  try {
    return fs.realpathSync(resolved);
  } catch {
    return resolved;
  }
}

/**
 * Resolve init install scope from `--scope` / default.
 *
 * - Explicit `user` / `project` → use as-is (`explicit: true`)
 * - Invalid value → throw
 * - Omitted → **project** (cwd), unless cwd === home (fall back to user)
 *
 * Local install location is decided only by the CLI; the store manifest's
 * `scope` field is ignored.
 */
export function resolveInitScope(
  rawScope: string | undefined,
  cwd: string,
  homeDir: string,
): { scope: Scope; projectRoot?: string; explicit: boolean; fallbackReason?: string } {
  const cwdResolved = resolveRealPath(cwd);
  const homeResolved = resolveRealPath(homeDir);
  const atHome = cwdResolved === homeResolved;

  if (rawScope !== undefined && rawScope !== '') {
    if (rawScope !== 'user' && rawScope !== 'project') {
      throw new Error(`Invalid scope "${rawScope}". Use "project" (default) or "user".`);
    }
    if (rawScope === 'project' && atHome) {
      throw new Error(
        'Cannot use --scope project in your home directory (paths would collide with user scope). ' +
        'cd to a project directory first, or omit --scope / use --scope user.',
      );
    }
    return {
      scope: rawScope,
      projectRoot: rawScope === 'project' ? cwdResolved : undefined,
      explicit: true,
    };
  }

  // Implicit default: project, with fallback when cwd is $HOME
  if (atHome) {
    return {
      scope: 'user',
      projectRoot: undefined,
      explicit: false,
      fallbackReason:
        'cwd is your home directory; using user scope to avoid path collision with ~/.dami-harness',
    };
  }

  return {
    scope: 'project',
    projectRoot: cwdResolved,
    explicit: false,
  };
}

/**
 * Resolve the project-local user-scope inheritance setting.
 *
 * An omitted flag preserves an existing project setting so additive re-init
 * operations such as `init --agent` do not silently disable inheritance.
 */
export function resolveInheritUserScope(
  scope: Scope,
  requested: boolean | undefined,
  existing: boolean | undefined,
): boolean | undefined {
  if (requested === true && scope !== 'project') {
    throw new Error('--inherit-user-scope can only be used with project scope.');
  }
  if (scope !== 'project') return undefined;
  return requested ?? existing;
}

function printScopeSummary(
  scope: Scope,
  projectRoot: string | undefined,
  explicit: boolean,
): void {
  const configPath = getConfigPath(scope, projectRoot);
  const baseDir = scope === 'project' ? (projectRoot ?? process.cwd()) : (process.env.HOME ?? '~');
  log.info(`Scope: ${scope}${scope === 'project' ? ` (${projectRoot})` : ''}`);
  log.info(`  config    → ${configPath}`);
  log.info(`  resources → ${baseDir}/.claude/skills, ...`);
  if (!explicit && scope === 'project') {
    log.info('  Tip: run with `--scope user` to install under your home directory (~/)');
  }
}

/** Walk up from dir looking for a `.git` entry (file or directory). */
async function isInsideGitRepo(dir: string): Promise<boolean> {
  let current = path.resolve(dir);
  for (;;) {
    if (await pathExists(path.join(current, '.git'))) return true;
    const parent = path.dirname(current);
    if (parent === current) return false;
    current = parent;
  }
}

/** Pick a non-interactive default username from the environment. */
function defaultUsername(): string {
  return process.env.USER ?? process.env.USERNAME ?? 'local';
}

export async function init(options: GlobalOptions & {
  scope?: string;
  agent?: string | string[];
  force?: boolean;
  inheritUserScope?: boolean;
}): Promise<void> {
  log.info('Initializing dami-harness...');

  // Step 0: Resolve scope (default project; only explicit --scope user → ~/ )
  let scope: Scope;
  let projectRoot: string | undefined;
  let explicit: boolean;
  let fallbackReason: string | undefined;
  try {
    ({ scope, projectRoot, explicit, fallbackReason } = resolveInitScope(
      options.scope,
      process.cwd(),
      process.env.HOME ?? '',
    ));
  } catch (e) {
    log.error((e as Error).message);
    process.exit(1);
    return;
  }
  const existingLocalConfig = await loadLocalConfigForScope(scope, projectRoot);
  let inheritUserScope: boolean | undefined;
  try {
    inheritUserScope = resolveInheritUserScope(
      scope,
      options.inheritUserScope,
      existingLocalConfig?.inheritUserScope,
    );
  } catch (e) {
    log.error((e as Error).message);
    process.exit(1);
    return;
  }
  if (fallbackReason) {
    log.warn(fallbackReason);
  }
  const harnessHome = getDamiHome(scope, projectRoot);
  printScopeSummary(scope, projectRoot, explicit);

  if (scope === 'project' && !(await isInsideGitRepo(process.cwd()))) {
    log.warn(`cwd is not inside a git repository; will create ${harnessHome}/`);
  }

  // Step 0.5: Re-init guard — warn if config already exists
  const existingConfigPath = getConfigPath(scope, projectRoot);
  if (await pathExists(existingConfigPath)) {
    log.warn(`dami-harness is already initialized for ${scope} scope at ${existingConfigPath}`);
    if (options.force) {
      log.info('Overwriting existing config (--force)');
    } else {
      const confirmed = await askConfirmation('Overwrite existing config? [y/N] ');
      if (!confirmed) {
        log.info('Aborted. Existing config is unchanged.');
        return;
      }
    }
  }

  // Step 1: Create the local resource store (plain directories — no git remote).
  const localPath = expandHome(path.join(harnessHome, 'store'));
  if (await pathExists(localPath)) {
    log.info(`Store already exists at ${localPath}, reusing it`);
  } else {
    log.info(`Store path: ${localPath}`);
  }
  await ensureDir(localPath);

  // Step 2: Store manifest (dami-harness.yaml)
  let teamConfig = await loadTeamConfig(localPath);
  if (!teamConfig) {
    log.warn('dami-harness.yaml not found in store. Creating default config...');
    const defaultConfig = YAML.stringify({
      team: 'local',
      description: 'dami-harness local resource store',
      sharing: {
        rules: { enforced: [] },
        docs: { localDir: scope === 'project' ? './.dami-harness/docs' : '~/.dami-harness/docs' },
      },
    });
    await writeFile(path.join(localPath, 'dami-harness.yaml'), defaultConfig);

    // Create standard directories
    for (const dir of ['skills', 'rules', 'docs', 'agents', 'hooks', 'mcp']) {
      await ensureDir(path.join(localPath, dir));
      const gitkeep = path.join(localPath, dir, '.gitkeep');
      if (!await pathExists(gitkeep)) {
        await writeFile(gitkeep, '');
      }
    }
    teamConfig = await loadTeamConfig(localPath);
  }

  // Step 3: Save local config
  const localConfig: LocalConfig = {
    repo: { localPath, remote: '' },
    username: defaultUsername(),
    scope,
    projectRoot,
    ...(inheritUserScope !== undefined ? { inheritUserScope } : {}),
  };

  // Persist --agent into enabledAgents (additive across runs)
  const requestedAgents = normalizeAgentList(options.agent);
  if (requestedAgents.length > 0) {
    const existing = await loadLocalConfigForScope(scope, projectRoot);
    const prev = existing?.enabledAgents ?? [];
    localConfig.enabledAgents = [...new Set([...prev, ...requestedAgents])];
    localConfig.disabledAgents = (existing?.disabledAgents ?? []).filter((t) => !requestedAgents.includes(t));
  }

  // Explicitly requested agents carry user intent to set the tool up: create
  // the tool root directory when missing so hook injection and resource
  // install below actually apply. Auto-detected (non-requested) agents keep
  // the skip-uninstalled behaviour.
  if (requestedAgents.length > 0) {
    const baseDir = resolveBaseDir(localConfig);
    for (const id of requestedAgents) {
      const skillsPath = teamConfig?.toolPaths?.[id]?.skills ?? `.${id}/skills`;
      const toolRoot = skillsPath.split(/[\\/]/)[0];
      const toolRootAbs = path.join(baseDir, toolRoot);
      if (!await pathExists(toolRootAbs)) {
        await ensureDir(toolRootAbs);
        log.info(`Created ${toolRoot}/ for agent "${id}"`);
      }
    }
  }

  await ensureDir(harnessHome);

  if (scope === 'project') {
    await saveLocalConfigForScope(localConfig, scope, projectRoot);
    log.success(`Local config saved to ${harnessHome}/config.yaml`);

    // Generate .gitignore for project scope to prevent local config from being committed
    const gitignorePath = path.join(harnessHome, '.gitignore');
    if (!await pathExists(gitignorePath)) {
      const gitignoreContent = [
        '# dami-harness local config (do not commit)',
        'config.yaml',
        'state.json',
        '.update-lock',
        '',
      ].join('\n');
      await writeFile(gitignorePath, gitignoreContent);
      log.debug('Generated .dami-harness/.gitignore for project scope');
    }
  } else {
    await ensureDir(DAMI_HOME);
    await saveLocalConfig(localConfig);
    log.success(`Local config saved to ${DAMI_HOME}/config.yaml`);
  }

  // Step 3.5: Invalidate sync cache so the next resource sync runs in full.
  try {
    const state = await loadStateForScope(scope, projectRoot);
    state.lastPullRev = null;
    await saveStateForScope(state, scope, projectRoot);
  } catch {
    // Non-critical: state file may not exist yet on first init
  }

  // Step 4: Inject built-in + store hooks into AI tools
  if (teamConfig) {
    const filterAgents = requestedAgents.length > 0 ? requestedAgents : undefined;
    await reconcileTeamHooksForConfig(teamConfig, localConfig, { filterAgents });

    // Step 5: Deploy CLI built-in resources (skills/rules/agents).
    // Recall-dependent payloads are skipped — the recall feature does not
    // exist in this local harness.
    await deployBuiltinSkills(teamConfig, localConfig, { skipRecall: true });
    await deployBuiltinRules(teamConfig, localConfig, { skipRecall: true });
    await deployBuiltinAgents(teamConfig, localConfig, { skipRecall: true });
    // Step 6: Install store skills into detected agent tools (per-tool install
    // is gated on the tool root existing, so uninstalled tools are untouched).
    const excluded = new Set(localConfig.excludedSkills ?? []);
    const skillsHandler = new SkillsHandler();
    for (const item of await skillsHandler.scanStoreForInstall(teamConfig, localConfig)) {
      if (excluded.has(item.name)) continue;
      await skillsHandler.installItem(item, teamConfig, localConfig);
    }
  }

  log.success('dami-harness initialized successfully!');
  log.info('Skills, rules and docs will sync from your local store into detected agent tools.');
  log.info('Run `dami-harness status` to check current config.');

  // Close the readline singleton so the process can exit cleanly.
  closePrompt();
}
