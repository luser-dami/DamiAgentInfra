import { homeDir } from './utils/home.js';
import path from 'node:path';
import { detectProjectConfig, loadLocalConfig, loadTeamConfig } from './config.js';
import { pathExists, readFileSafe } from './utils/fs.js';
import { log } from './utils/logger.js';
import type { GlobalOptions, Scope } from './types.js';
import {
  HarnessConfigSchema,
  resolveBaseDir,
  type HarnessConfig,
} from './types.js';
import { DAMI_HOOK_SUBCOMMANDS } from './hooks.js';

interface Check {
  name: string;
  check: () => Promise<boolean>;
  fix?: string;
}

/**
 * Build hook checks only for tools whose settings parent directory already
 * exists (i.e. the tool is installed). Tools that are not installed are skipped.
 */
async function buildHookChecks(toolPaths: HarnessConfig['toolPaths'], baseDir: string): Promise<Check[]> {
  const checks: Check[] = [];
  for (const [tool, paths] of Object.entries(toolPaths)) {
    if (!paths.settings) continue;
    const settingsPath = path.join(baseDir, paths.settings);
    const parentDir = path.dirname(settingsPath);
    if (!await pathExists(parentDir)) continue;
    checks.push({
      name: `dami-harness hooks in ${tool} settings`,
      check: async () => {
        if (!await pathExists(settingsPath)) return false;
        const content = await readFileSafe(settingsPath);
        if (!content) return false;

        const missing = DAMI_HOOK_SUBCOMMANDS.filter(
          (sub) => !content.includes(`dami-harness ${sub}`),
        );
        return missing.length === 0;
      },
      fix: 'Run `dami-harness hooks inject` to inject/update hooks',
    });
  }
  return checks;
}

export interface DoctorResult {
  name: string;
  ok: boolean;
  fix?: string;
}

export async function doctor(options: GlobalOptions): Promise<DoctorResult[]> {
  log.info('Running diagnostics...\n');
  const projectConfig = await detectProjectConfig();
  const localConfig = projectConfig ?? (await loadLocalConfig());
  const scope: Scope = localConfig?.scope ?? 'user';
  const configPathLabel = projectConfig
    ? `${projectConfig.projectRoot}/.dami-harness/config.yaml`
    : '~/.dami-harness/config.yaml';

  log.info(`  Scope: ${scope}${scope === 'project' && localConfig?.projectRoot ? ` (${localConfig.projectRoot})` : ''}\n`);

  // Try to load the store manifest for dynamic tool paths
  let teamConfig: HarnessConfig | null = null;
  if (localConfig) {
    teamConfig = await loadTeamConfig(localConfig.repo.localPath);
  }
  // Fall back to schema defaults if the store manifest is unavailable
  const toolPaths = teamConfig?.toolPaths ?? HarnessConfigSchema.shape.toolPaths.parse(undefined);
  const baseDir = localConfig ? resolveBaseDir(localConfig) : (homeDir() ?? '');

  const checks: Check[] = [];

  checks.push(
    {
      name: `Local config exists (${configPathLabel})`,
      check: async () => localConfig !== null,
      fix: 'Run `dami-harness init` to initialize',
    },
    {
      name: 'Resource store exists locally',
      check: async () => {
        if (!localConfig) return false;
        return pathExists(localConfig.repo.localPath);
      },
      fix: 'Run `dami-harness init` to create the local store',
    },
    {
      name: 'Team config (dami-harness.yaml) is valid',
      check: async () => {
        if (!localConfig) return false;
        const config = await loadTeamConfig(localConfig.repo.localPath);
        return config !== null;
      },
      fix: 'Check dami-harness.yaml in the store for syntax errors',
    },
    ...await buildHookChecks(toolPaths, baseDir),
  );

  const results: DoctorResult[] = [];
  for (const { name, check, fix } of checks) {
    const ok = await check();
    results.push({ name, ok, ...(ok || !fix ? {} : { fix }) });
    if (ok) {
      log.info(`  ✔ ${name}`);
    } else {
      log.info(`  ✖ ${name}`);
      if (fix) log.info(`    → ${fix}`);
    }
  }
  const allPassed = results.every((r) => r.ok);

  log.info('');
  if (allPassed) {
    log.success('All checks passed!');
  } else {
    log.warn('Some checks failed. See suggestions above.');
  }
  return results;
}
