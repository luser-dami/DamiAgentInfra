import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import path from 'node:path';
import os from 'node:os';
import fse from 'fs-extra';

import { HarnessConfigSchema, type HarnessConfig, type LocalConfig } from '../types.js';
import { KNOWN_AGENTS, detectInstalledAgents } from '../known-agents.js';
import { SkillsHandler } from '../resources/skills.js';
import { ResourceHandler } from '../resources/base.js';

// OMP (Oh My Pi) agent target: detection via .omp/ presence and the
// skills → .omp/skills/<name>/SKILL.md mapping observed in a live .omp/ tree.

describe('OMP agent support', () => {
  it('known-agents registry includes omp with the .omp/skills layout', () => {
    const omp = KNOWN_AGENTS.find((a) => a.id === 'omp');
    expect(omp).toBeDefined();
    expect(omp?.skillsPath).toBe('.omp/skills');
  });

  it('toolPaths schema defaults include omp skills + AGENTS.md manual', () => {
    const config = HarnessConfigSchema.parse({ team: 'test' });
    expect(config.toolPaths.omp).toEqual({
      skills: '.omp/skills',
      claudemd: '.omp/AGENTS.md',
    });
  });

  describe('detection via .omp/ presence', () => {
    let tmpDir: string;
    let homeDir: string;
    let localConfig: LocalConfig;
    let teamConfig: HarnessConfig;

    beforeEach(async () => {
      tmpDir = await fse.mkdtemp(path.join(os.tmpdir(), 'dami-harness-omp-'));
      homeDir = path.join(tmpDir, 'home');
      await fse.ensureDir(homeDir);
      vi.stubEnv('HOME', homeDir);

      localConfig = {
        repo: { localPath: path.join(tmpDir, 'store'), remote: '' },
        username: 'testuser',
        scope: 'user',
      };
      teamConfig = HarnessConfigSchema.parse({ team: 'test' });
    });

    afterEach(async () => {
      vi.unstubAllEnvs();
      await fse.remove(tmpDir);
    });

    it('reports omp installed when ~/.omp exists', async () => {
      await fse.ensureDir(path.join(homeDir, '.omp'));
      const agents = await detectInstalledAgents(localConfig, teamConfig);
      const omp = agents.find((a) => a.id === 'omp');
      expect(omp?.installed).toBe(true);
      expect(omp?.absoluteSkillsPath.replaceAll('\\', '/')).toBe(`${homeDir.replaceAll('\\', '/')}/.omp/skills`);
    });

    it('reports omp not installed when ~/.omp is absent', async () => {
      const agents = await detectInstalledAgents(localConfig, teamConfig);
      expect(agents.find((a) => a.id === 'omp')?.installed).toBe(false);
    });

    it('isToolInstalled detects .omp via the first path segment', async () => {
      expect(await ResourceHandler.isToolInstalled('.omp/skills', homeDir)).toBe(false);
      await fse.ensureDir(path.join(homeDir, '.omp'));
      expect(await ResourceHandler.isToolInstalled('.omp/skills', homeDir)).toBe(true);
    });
  });

  describe('skills mapping → .omp/skills/<name>/SKILL.md', () => {
    let tmpDir: string;
    let homeDir: string;
    let repoPath: string;
    let teamConfig: HarnessConfig;
    let localConfig: LocalConfig;

    beforeEach(async () => {
      tmpDir = await fse.mkdtemp(path.join(os.tmpdir(), 'dami-harness-omp-skill-'));
      homeDir = path.join(tmpDir, 'home');
      repoPath = path.join(tmpDir, 'store');
      await fse.ensureDir(path.join(repoPath, 'skills'));
      // OMP is "installed"; claude deliberately is not.
      await fse.ensureDir(path.join(homeDir, '.omp'));
      vi.stubEnv('HOME', homeDir);

      teamConfig = HarnessConfigSchema.parse({ team: 'test' });
      localConfig = {
        repo: { localPath: repoPath, remote: '' },
        username: 'testuser',
        scope: 'user',
      };

      const skillDir = path.join(repoPath, 'skills', 'test-skill');
      await fse.ensureDir(skillDir);
      await fse.writeFile(path.join(skillDir, 'SKILL.md'), '# Test Skill');
    });

    afterEach(async () => {
      vi.unstubAllEnvs();
      await fse.remove(tmpDir);
    });

    it('installs a store skill into .omp/skills/<name>/SKILL.md', async () => {
      const handler = new SkillsHandler();
      await handler.installItem(
        {
          name: 'test-skill',
          type: 'skills',
          sourcePath: path.join(repoPath, 'skills', 'test-skill'),
          relativePath: 'skills/test-skill',
        },
        teamConfig,
        localConfig,
      );

      const installed = path.join(homeDir, '.omp', 'skills', 'test-skill', 'SKILL.md');
      expect(await fse.pathExists(installed)).toBe(true);
      expect(await fse.readFile(installed, 'utf-8')).toContain('# Test Skill');
      // Not installed tools get nothing.
      expect(await fse.pathExists(path.join(homeDir, '.claude'))).toBe(false);
    });

    it('scanStoreForInstall finds skills in the store for omp', async () => {
      const handler = new SkillsHandler();
      const items = await handler.scanStoreForInstall(teamConfig, localConfig);
      expect(items.map((i) => i.name)).toContain('test-skill');
    });
  });
});
