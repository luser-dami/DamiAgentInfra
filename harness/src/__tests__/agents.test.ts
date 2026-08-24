import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import path from 'node:path';
import os from 'node:os';
import fse from 'fs-extra';

vi.mock('../utils/logger.js', () => ({
  log: {
    info: vi.fn(),
    success: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
    dim: vi.fn(),
  },
  spinner: vi.fn(() => ({
    start: vi.fn().mockReturnThis(),
    succeed: vi.fn().mockReturnThis(),
    fail: vi.fn().mockReturnThis(),
  })),
}));

import { AgentsHandler } from '../resources/agents.js';
import type { HarnessConfig, LocalConfig } from '../types.js';

/**
 * Build a minimal HarnessConfig with the given toolPaths.
 * Returns a proxy object cast to HarnessConfig — the handler only reads
 * `toolPaths`, so other fields can stay shallow.
 */
function buildTeamConfig(
  toolPaths: HarnessConfig['toolPaths'],
): HarnessConfig {
  return {
    team: 'test',
    description: '',
    repo: 'https://example.com/test/repo.git',
    provider: 'tgit' as const,
    reviewers: [],
    sharing: {
      skills: {},
      rules: { enforced: [] },
      docs: { localDir: '' },
      env: { injectShellProfile: true },
    },
    toolPaths,
  } as HarnessConfig;
}

describe('AgentsHandler — Phase 1 push/pull/remove', () => {
  let tmpDir: string;
  let homeDir: string;
  let repoPath: string;
  let handler: AgentsHandler;
  let teamConfig: HarnessConfig;
  let localConfig: LocalConfig;

  beforeEach(async () => {
    tmpDir = await fse.mkdtemp(path.join(os.tmpdir(), 'dami-harness-agents-test-'));
    homeDir = path.join(tmpDir, 'home');
    repoPath = path.join(tmpDir, 'team-repo');

    await fse.ensureDir(path.join(repoPath, 'agents'));
    await fse.ensureDir(path.join(homeDir, '.claude', 'agents'));
    await fse.ensureDir(path.join(homeDir, '.codebuddy', 'agents'));
    // cursor intentionally has no agents directory — Tier-3 tool

    vi.stubEnv('HOME', homeDir);

    handler = new AgentsHandler();

    teamConfig = buildTeamConfig({
      claude: { skills: '.claude/skills', rules: '.claude/rules', agents: '.claude/agents' },
      codebuddy: { skills: '.codebuddy/skills', rules: '.codebuddy/rules', agents: '.codebuddy/agents' },
      // No agents path: should be silently skipped
      cursor: { skills: '.cursor/skills', rules: '.cursor/rules' },
    });

    localConfig = {
      repo: { localPath: repoPath, remote: 'https://example.com/test/repo.git' },
      username: 'testuser',
      additionalRoles: [],
      scope: 'user',
    };
  });

  afterEach(async () => {
    vi.unstubAllEnvs();
    await fse.remove(tmpDir);
  });

  // ── scanStoreForInstall ─────────────────────────────────────

  it('scanStoreForInstall returns *.md files from team repo agents/', async () => {
    await fse.writeFile(path.join(repoPath, 'agents', 'code-reviewer.md'), '# code reviewer');
    await fse.writeFile(path.join(repoPath, 'agents', 'doc-writer.md'), '# doc writer');
    // Non-md files must be ignored
    await fse.writeFile(path.join(repoPath, 'agents', 'README.txt'), 'should be ignored');

    const items = await handler.scanStoreForInstall(teamConfig, localConfig);
    const names = items.map((i) => i.name).sort();
    expect(names).toEqual(['code-reviewer', 'doc-writer']);
    expect(items.every((i) => i.type === 'agents')).toBe(true);
  });

  it('scanStoreForInstall returns empty when team repo has no agents directory', async () => {
    await fse.remove(path.join(repoPath, 'agents'));
    const items = await handler.scanStoreForInstall(teamConfig, localConfig);
    expect(items).toEqual([]);
  });

  // ── installItem ────────────────────────────────────────────

  it('installItem deploys *.md to every tool whose toolPaths.agents is configured', async () => {
    const srcPath = path.join(repoPath, 'agents', 'helper.md');
    await fse.writeFile(srcPath, '# helper agent');

    await handler.installItem(
      {
        name: 'helper',
        type: 'agents',
        sourcePath: srcPath,
        relativePath: 'agents/helper.md',
      },
      teamConfig,
      localConfig,
    );

    expect(await fse.pathExists(path.join(homeDir, '.claude/agents/helper.md'))).toBe(true);
    expect(await fse.pathExists(path.join(homeDir, '.codebuddy/agents/helper.md'))).toBe(true);
  });

  it('installItem silently skips tools without agents path (cursor/codex/etc.)', async () => {
    const srcPath = path.join(repoPath, 'agents', 'helper.md');
    await fse.writeFile(srcPath, '# helper agent');

    // cursor only has skills/rules, no agents — must not blow up
    await handler.installItem(
      {
        name: 'helper',
        type: 'agents',
        sourcePath: srcPath,
        relativePath: 'agents/helper.md',
      },
      teamConfig,
      localConfig,
    );

    expect(await fse.pathExists(path.join(homeDir, '.cursor/agents/helper.md'))).toBe(false);
  });

  it('installItem skips tools that are not installed (no tool root dir)', async () => {
    // Add another tool whose root does NOT exist on the user machine
    const cfg = buildTeamConfig({
      claude: { skills: '.claude/skills', agents: '.claude/agents' },
      'claude-internal': { skills: '.claude-internal/skills', agents: '.claude-internal/agents' },
    });
    const srcPath = path.join(repoPath, 'agents', 'helper.md');
    await fse.writeFile(srcPath, '# helper');

    await handler.installItem(
      { name: 'helper', type: 'agents', sourcePath: srcPath, relativePath: 'agents/helper.md' },
      cfg,
      localConfig,
    );

    expect(await fse.pathExists(path.join(homeDir, '.claude/agents/helper.md'))).toBe(true);
    expect(await fse.pathExists(path.join(homeDir, '.claude-internal/agents/helper.md'))).toBe(false);
  });

  // ── scanLocalForCollect ────────────────────────────────────

  it('scanLocalForCollect detects a modified agent across tool dirs as "modified"', async () => {
    await fse.writeFile(path.join(repoPath, 'agents', 'shared.md'), 'team version');
    await fse.writeFile(path.join(homeDir, '.claude/agents', 'shared.md'), 'local edits');

    const items = await handler.scanLocalForCollect(teamConfig, localConfig);
    const item = items.find((i) => i.name === 'shared');
    expect(item).toBeDefined();
    expect(item!.status).toBe('modified');
  });

  it('scanLocalForCollect detects a brand-new local agent as "new"', async () => {
    await fse.writeFile(path.join(homeDir, '.claude/agents', 'brand-new.md'), '# brand new');
    const items = await handler.scanLocalForCollect(teamConfig, localConfig);
    const item = items.find((i) => i.name === 'brand-new');
    expect(item).toBeDefined();
    expect(item!.status).toBe('new');
  });

  it('scanLocalForCollect ignores local copies identical to team repo', async () => {
    await fse.writeFile(path.join(repoPath, 'agents', 'same.md'), 'identical');
    await fse.writeFile(path.join(homeDir, '.claude/agents', 'same.md'), 'identical');

    const items = await handler.scanLocalForCollect(teamConfig, localConfig);
    expect(items.find((i) => i.name === 'same')).toBeUndefined();
  });

  it('scanLocalForCollect excludes built-in CLI agents (e.g. dami-harness-recall)', async () => {
    await fse.writeFile(
      path.join(homeDir, '.claude/agents', 'dami-harness-recall.md'),
      '# managed by CLI — must not be pushed',
    );
    const items = await handler.scanLocalForCollect(teamConfig, localConfig);
    expect(items.find((i) => i.name === 'dami-harness-recall')).toBeUndefined();
  });

  // ── collectItem ────────────────────────────────────────────

  it('collectItem copies the local md file into team-repo/agents/', async () => {
    const localFile = path.join(homeDir, '.claude/agents', 'pushed.md');
    await fse.writeFile(localFile, '# pushed agent');

    await handler.collectItem(
      { name: 'pushed', type: 'agents', sourcePath: localFile, relativePath: 'agents/pushed.md' },
      teamConfig,
      localConfig,
    );

    const teamFile = path.join(repoPath, 'agents', 'pushed.md');
    expect(await fse.pathExists(teamFile)).toBe(true);
    expect((await fse.readFile(teamFile, 'utf8'))).toBe('# pushed agent');
  });

  // ── removeItem + tombstone ──────────────────────────────

  it('removeItem deletes from team repo and all tool agents/ dirs and writes a tombstone', async () => {
    await fse.writeFile(path.join(repoPath, 'agents', 'old.md'), 'old');
    await fse.writeFile(path.join(homeDir, '.claude/agents', 'old.md'), 'old');
    await fse.writeFile(path.join(homeDir, '.codebuddy/agents', 'old.md'), 'old');

    const removed = await handler.removeItem('old', teamConfig, localConfig);

    expect(await fse.pathExists(path.join(repoPath, 'agents', 'old.md'))).toBe(false);
    expect(await fse.pathExists(path.join(homeDir, '.claude/agents', 'old.md'))).toBe(false);
    expect(await fse.pathExists(path.join(homeDir, '.codebuddy/agents', 'old.md'))).toBe(false);
    expect(removed.length).toBeGreaterThanOrEqual(3);

    // Tombstone must be present so the agent is not re-pushed if a stale local
    // copy reappears.
    const tombstone = await fse.readFile(path.join(repoPath, 'agents', '.removed'), 'utf8');
    expect(tombstone.split('\n').map((l) => l.trim())).toContain('old');
  });

  it('scanLocalForCollect respects tombstones (skips removed items)', async () => {
    await fse.writeFile(path.join(repoPath, 'agents', '.removed'), 'ghost\n');
    await fse.writeFile(path.join(homeDir, '.claude/agents', 'ghost.md'), '# revived');

    const items = await handler.scanLocalForCollect(teamConfig, localConfig);
    expect(items.find((i) => i.name === 'ghost')).toBeUndefined();
  });
});
