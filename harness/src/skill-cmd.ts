import path from 'node:path';
import { autoDetectInit } from './config.js';
import { log } from './utils/logger.js';
import { listDirs, pathExists } from './utils/fs.js';
import { SkillsHandler } from './resources/skills.js';
import { loadTagsConfig } from './utils/tags.js';
import {
  buildClassifyContext,
  classifySkill,
  formatSkillSource,
  readSkillDescription,
  truncate,
  type SkillSource,
} from './agent-skills.js';
import { detectInstalledAgents, type ResolvedAgent } from './known-agents.js';
import type { GlobalOptions, LocalConfig } from './types.js';

const DESCRIPTION_MAX = 160;

interface ResolvedSkill {
  name: string;
  /** Path used to read SKILL.md, contributors and description. */
  primaryPath: string;
  /** Where the primary copy was discovered. */
  primaryOrigin: 'team' | 'agent';
  /** Optional namespace if found in the team repo. */
  namespace?: string;
}

/**
 * `dami-harness skill show <name>` — print metadata about a single
 * skill: source classification, contributors, namespace, tags
 * and which installed agents currently host it.
 *
 * The full SKILL.md body is intentionally not rendered; users
 * who need the markdown can `cat` it directly using the path
 * we print under "Repo path" or "Installed in".
 */
export async function skillShow(name: string, options: GlobalOptions): Promise<void> {
  const { localConfig, teamConfig } = await autoDetectInit();

  const agents = await detectInstalledAgents(localConfig, teamConfig);
  const resolved = await locateSkill(name, localConfig, agents);
  if (!resolved) {
    log.error(`Skill "${name}" not found in team repo or any installed agent.`);
    log.dim('Try `dami-harness list --source all` to see available skills.');
    process.exitCode = 1;
    return;
  }

  const ctx = await buildClassifyContext(localConfig);
  const source = classifySkill(name, ctx);

  const description = truncate(await readSkillDescription(path.join(resolved.primaryPath, 'SKILL.md')), DESCRIPTION_MAX);
  const contributors = await SkillsHandler.readContributors(resolved.primaryPath);

  const tagsConfig = await loadTagsConfig(localConfig.repo.localPath);
  const tags = tagsConfig?.skills?.[name] ?? [];

  const installedIn = await collectInstalledAgents(name, agents);

  printSkillCard({
    name,
    source,
    namespace: resolved.namespace ?? (source.kind === 'team' ? source.namespace : undefined),
    description,
    contributors,
    tags,
    primaryPath: resolved.primaryPath,
    primaryOrigin: resolved.primaryOrigin,
    installedIn,
  });

  if (options.verbose) {
    console.log('');
    console.log(`  Verbose: SKILL.md path is ${path.join(resolved.primaryPath, 'SKILL.md')}`);
  }
}

async function locateSkill(
  name: string,
  localConfig: LocalConfig,
  agents: ResolvedAgent[],
): Promise<ResolvedSkill | null> {
  const teamSkillsDir = path.join(localConfig.repo.localPath, 'skills');

  // 1. Flat layout in team repo
  const flat = path.join(teamSkillsDir, name);
  if (await pathExists(path.join(flat, 'SKILL.md'))) {
    return { name, primaryPath: flat, primaryOrigin: 'team' };
  }

  // 2. Namespaced layout in team repo
  if (await pathExists(teamSkillsDir)) {
    const namespaces = await listDirs(teamSkillsDir);
    for (const ns of namespaces) {
      const candidate = path.join(teamSkillsDir, ns, name);
      if (await pathExists(path.join(candidate, 'SKILL.md'))) {
        return { name, primaryPath: candidate, primaryOrigin: 'team', namespace: ns };
      }
    }
  }

  // 3. First installed agent that has the skill
  for (const agent of agents) {
    if (!agent.installed) continue;
    const candidate = path.join(agent.absoluteSkillsPath, name);
    if (await pathExists(path.join(candidate, 'SKILL.md'))) {
      return { name, primaryPath: candidate, primaryOrigin: 'agent' };
    }
  }

  return null;
}

async function collectInstalledAgents(
  name: string,
  agents: ResolvedAgent[],
): Promise<Array<{ agent: ResolvedAgent; path: string }>> {
  const matches: Array<{ agent: ResolvedAgent; path: string }> = [];
  for (const agent of agents) {
    if (!agent.installed) continue;
    const skillDir = path.join(agent.absoluteSkillsPath, name);
    if (await pathExists(path.join(skillDir, 'SKILL.md'))) {
      matches.push({ agent, path: skillDir });
    }
  }
  return matches;
}

interface SkillCard {
  name: string;
  source: SkillSource;
  namespace?: string;
  description: string;
  contributors: string[];
  tags: string[];
  primaryPath: string;
  primaryOrigin: 'team' | 'agent';
  installedIn: Array<{ agent: ResolvedAgent; path: string }>;
}

function printSkillCard(card: SkillCard): void {
  const bar = '='.repeat(60);
  console.log('');
  console.log(bar);
  console.log(`  skill: ${card.name}`);
  console.log(bar);
  console.log('');

  console.log(`  Source       : ${formatSkillSource(card.source)}`);
  if (card.namespace) {
    console.log(`  Namespace    : ${card.namespace}`);
  }
  console.log(`  Description  : ${card.description || '(none)'}`);
  console.log(`  Contributors : ${card.contributors.length > 0 ? card.contributors.join(', ') : '(none)'}`);
  console.log(`  Tags         : ${card.tags.length > 0 ? card.tags.join(', ') : '(none)'}`);
  console.log(`  ${card.primaryOrigin === 'team' ? 'Repo path  ' : 'Source path'}  : ${card.primaryPath}/`);

  if (card.installedIn.length === 0) {
    console.log('  Installed in : (not installed in any agent yet)');
  } else {
    const first = card.installedIn[0];
    console.log(`  Installed in : ${first.agent.id} (${first.path})`);
    for (let i = 1; i < card.installedIn.length; i++) {
      const entry = card.installedIn[i];
      console.log(`                 ${entry.agent.id} (${entry.path})`);
    }
  }
  console.log('');
}
