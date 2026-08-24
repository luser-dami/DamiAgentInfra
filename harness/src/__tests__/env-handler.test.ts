import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import path from 'node:path';
import os from 'node:os';
import fse from 'fs-extra';
import YAML from 'yaml';
import { EnvHandler } from '../resources/env.js';
import { DAMI_ENV_START, DAMI_ENV_END } from '../types.js';
import type { HarnessConfig, LocalConfig, ResourceItem } from '../types.js';

vi.mock('../utils/logger.js', () => ({
  log: {
    info: vi.fn(),
    success: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
    dim: vi.fn(),
  },
}));

describe('EnvHandler', () => {
  let handler: EnvHandler;
  let tmpDir: string;
  let homeDir: string;
  let repoPath: string;
  let teamConfig: HarnessConfig;
  let localConfig: LocalConfig;

  beforeEach(async () => {
    handler = new EnvHandler();
    tmpDir = await fse.mkdtemp(path.join(os.tmpdir(), 'dami-harness-env-test-'));
    homeDir = path.join(tmpDir, 'home');
    repoPath = path.join(tmpDir, 'team-repo');

    await fse.ensureDir(path.join(repoPath, 'env'));
    await fse.ensureDir(path.join(homeDir, '.dami-harness'));

    vi.stubEnv('HOME', homeDir);
    vi.stubEnv('SHELL', '/bin/bash');

    teamConfig = {
      team: 'test',
      description: '',
      sharing: {
        skills: {},
        rules: { enforced: [] },
        docs: { localDir: '' },
        env: { injectShellProfile: true },
      },
      toolPaths: {},
    };

    localConfig = {
      repo: { localPath: repoPath, remote: 'https://git.woa.com/test/repo.git' },
      username: 'testuser',
      updatePolicy: 'auto',
additionalRoles: [],
scope: 'user',
    };
  });

  afterEach(async () => {
    vi.unstubAllEnvs();
    await fse.remove(tmpDir);
  });

  // ─── scanStoreForInstall ─────────────────────────────────────

  describe('scanStoreForInstall', () => {
    it('should return empty array when env.yaml does not exist', async () => {
      const items = await handler.scanStoreForInstall(teamConfig, localConfig);
      expect(items).toEqual([]);
    });

    it('should return resource item when env.yaml exists', async () => {
      const envYamlPath = path.join(repoPath, 'env', 'env.yaml');
      await fse.writeFile(envYamlPath, YAML.stringify({ variables: [] }));

      const items = await handler.scanStoreForInstall(teamConfig, localConfig);
      expect(items).toHaveLength(1);
      expect(items[0].name).toBe('env.yaml');
      expect(items[0].type).toBe('env');
      expect(items[0].relativePath).toBe('env/env.yaml');
    });
  });

  // ─── parseEnvYaml ────────────────────────────────────────

  describe('parseEnvYaml', () => {
    it('should parse valid env.yaml', async () => {
      const envYamlPath = path.join(repoPath, 'env', 'env.yaml');
      await fse.writeFile(envYamlPath, YAML.stringify({
        variables: [
          { key: 'FOO', value: 'bar', description: 'test var' },
          { key: 'BAZ', value: 'qux' },
        ],
      }));

      const result = await handler.parseEnvYaml(envYamlPath);
      expect(result.variables).toHaveLength(2);
      expect(result.variables[0]).toEqual({ key: 'FOO', value: 'bar', description: 'test var' });
      expect(result.variables[1]).toEqual({ key: 'BAZ', value: 'qux' });
    });

    it('should return empty variables for non-existent file', async () => {
      const result = await handler.parseEnvYaml('/no/such/file.yaml');
      expect(result.variables).toEqual([]);
    });

    it('should return empty variables for invalid yaml', async () => {
      const envYamlPath = path.join(repoPath, 'env', 'env.yaml');
      await fse.writeFile(envYamlPath, ':::invalid yaml[[[');

      const result = await handler.parseEnvYaml(envYamlPath);
      expect(result.variables).toEqual([]);
    });

    it('should default variables to empty array when missing', async () => {
      const envYamlPath = path.join(repoPath, 'env', 'env.yaml');
      await fse.writeFile(envYamlPath, YAML.stringify({ other: 'data' }));

      const result = await handler.parseEnvYaml(envYamlPath);
      expect(result.variables).toEqual([]);
    });
  });

  // ─── writeEnvYaml ────────────────────────────────────────

  describe('writeEnvYaml', () => {
    it('should write env.yaml correctly', async () => {
      const envYamlPath = path.join(repoPath, 'env', 'env.yaml');
      const envConfig = {
        variables: [{ key: 'MY_VAR', value: 'hello' }],
      };

      await handler.writeEnvYaml(envYamlPath, envConfig);

      const content = await fse.readFile(envYamlPath, 'utf-8');
      const parsed = YAML.parse(content);
      expect(parsed.variables).toHaveLength(1);
      expect(parsed.variables[0].key).toBe('MY_VAR');
      expect(parsed.variables[0].value).toBe('hello');
    });

    it('should create parent directories if needed', async () => {
      const envYamlPath = path.join(tmpDir, 'new', 'dir', 'env.yaml');
      await handler.writeEnvYaml(envYamlPath, { variables: [] });
      expect(await fse.pathExists(envYamlPath)).toBe(true);
    });
  });

  // ─── countEnvVars ────────────────────────────────────────

  describe('countEnvVars', () => {
    it('should count variables in env.yaml', async () => {
      const envYamlPath = path.join(repoPath, 'env', 'env.yaml');
      await fse.writeFile(envYamlPath, YAML.stringify({
        variables: [
          { key: 'A', value: '1' },
          { key: 'B', value: '2' },
          { key: 'C', value: '3' },
        ],
      }));

      const count = await handler.countEnvVars(envYamlPath);
      expect(count).toBe(3);
    });

    it('should return 0 for non-existent file', async () => {
      const count = await handler.countEnvVars('/no/such/file.yaml');
      expect(count).toBe(0);
    });

    it('should return 0 for invalid yaml', async () => {
      const envYamlPath = path.join(repoPath, 'env', 'env.yaml');
      await fse.writeFile(envYamlPath, ':::bad');

      const count = await handler.countEnvVars(envYamlPath);
      expect(count).toBe(0);
    });
  });

  // ─── generateShellBlock ──────────────────────────────────

  describe('generateShellBlock', () => {
    it('should generate source line block with markers', () => {
      const block = handler.generateShellBlock('~/.dami-harness');

      expect(block).toContain(DAMI_ENV_START);
      expect(block).toContain(DAMI_ENV_END);
      expect(block).toContain('# DO NOT EDIT: This section is auto-managed by dami-harness');
      expect(block).toContain('[ -f ~/.dami-harness/env.sh ] && source ~/.dami-harness/env.sh');
      // Should NOT contain inline export lines
      expect(block).not.toMatch(/^export /m);
    });
  });

  // ─── generateEnvFile ─────────────────────────────────────

  describe('generateEnvFile', () => {
    it('should generate export lines for env.sh', () => {
      const content = handler.generateEnvFile([
        { key: 'API_URL', value: 'https://example.com' },
        { key: 'TOKEN', value: 'abc123' },
      ]);

      expect(content).toBe(
        "export API_URL='https://example.com'\nexport TOKEN='abc123'\n",
      );
    });

    it('should shell-quote values containing shell metacharacters', () => {
      // Values flow from the team repo's env/env.yaml into env.sh, which every
      // member sources from their shell profile. Quotes / `$` / backticks must
      // be taken literally and must not break or inject into the sourced shell.
      const content = handler.generateEnvFile([
        { key: 'CONN', value: 'a"b$c' },
        { key: 'GREETING', value: "it's" },
      ]);

      expect(content).toBe(
        "export CONN='a\"b$c'\nexport GREETING='it'\\''s'\n",
      );
      // No raw double-quote wrapping that the old code produced.
      expect(content).not.toContain('="a');
    });

    it('should return just a newline for empty variables', () => {
      const content = handler.generateEnvFile([]);
      expect(content).toBe('\n');
    });
  });

  // ─── installItem ────────────────────────────────────────────

  describe('installItem', () => {
    const envYaml = {
      variables: [
        { key: 'TGIT_API_BASE', value: 'https://git.woa.com/api/v3', description: 'TGit API' },
        { key: 'MODEL_ENDPOINT', value: 'https://api.example.com' },
      ],
    };

    let item: ResourceItem;

    beforeEach(async () => {
      const envYamlPath = path.join(repoPath, 'env', 'env.yaml');
      await fse.writeFile(envYamlPath, YAML.stringify(envYaml));
      item = {
        name: 'env.yaml',
        type: 'env',
        sourcePath: envYamlPath,
        relativePath: 'env/env.yaml',
      };
    });

    it('should write backup to ~/.dami-harness/env in KEY=VALUE format', async () => {
      await handler.installItem(item, teamConfig, localConfig);

      const backupPath = path.join(homeDir, '.dami-harness', 'env');
      expect(await fse.pathExists(backupPath)).toBe(true);
      const content = await fse.readFile(backupPath, 'utf-8');
      expect(content).toContain('TGIT_API_BASE=https://git.woa.com/api/v3');
      expect(content).toContain('MODEL_ENDPOINT=https://api.example.com');
    });

    it('should write ~/.dami-harness/env.sh with export lines', async () => {
      await handler.installItem(item, teamConfig, localConfig);

      const envShPath = path.join(homeDir, '.dami-harness', 'env.sh');
      expect(await fse.pathExists(envShPath)).toBe(true);
      const content = await fse.readFile(envShPath, 'utf-8');
      expect(content).toContain("export TGIT_API_BASE='https://git.woa.com/api/v3'");
      expect(content).toContain("export MODEL_ENDPOINT='https://api.example.com'");
    });

    it('should inject source line into shell profile (bash)', async () => {
      vi.stubEnv('SHELL', '/bin/bash');
      const bashrcPath = path.join(homeDir, '.bashrc');
      await fse.writeFile(bashrcPath, '# existing config\nexport PATH=$PATH\n');

      await handler.installItem(item, teamConfig, localConfig);

      const content = await fse.readFile(bashrcPath, 'utf-8');
      expect(content).toContain('# existing config');
      expect(content).toContain(DAMI_ENV_START);
      expect(content).toContain(`[ -f ${homeDir}${path.sep}.dami-harness/env.sh ] && source ${homeDir}${path.sep}.dami-harness/env.sh`);
      expect(content).toContain(DAMI_ENV_END);
      // Should NOT have inline export lines in the profile
      expect(content).not.toContain('export TGIT_API_BASE');
    });

    it('should inject source line into .zshrc for zsh users', async () => {
      vi.stubEnv('SHELL', '/bin/zsh');
      const zshrcPath = path.join(homeDir, '.zshrc');
      await fse.writeFile(zshrcPath, '# zsh config\n');

      await handler.installItem(item, teamConfig, localConfig);

      const content = await fse.readFile(zshrcPath, 'utf-8');
      expect(content).toContain(DAMI_ENV_START);
      expect(content).toContain(`[ -f ${homeDir}${path.sep}.dami-harness/env.sh ] && source ${homeDir}${path.sep}.dami-harness/env.sh`);
    });

    it('should idempotently replace existing block (including old-style with exports)', async () => {
      vi.stubEnv('SHELL', '/bin/bash');
      const bashrcPath = path.join(homeDir, '.bashrc');

      // Existing content with OLD-style env block (inline exports)
      const existingContent = [
        '# my config',
        DAMI_ENV_START,
        '# DO NOT EDIT: This section is auto-managed by dami-harness',
        'export OLD_VAR="old_value"',
        DAMI_ENV_END,
        '# other config',
      ].join('\n');
      await fse.writeFile(bashrcPath, existingContent);

      await handler.installItem(item, teamConfig, localConfig);

      const content = await fse.readFile(bashrcPath, 'utf-8');
      // Old inline export should be gone
      expect(content).not.toContain('OLD_VAR');
      // Source line should be present instead
      expect(content).toContain(`[ -f ${homeDir}${path.sep}.dami-harness/env.sh ] && source ${homeDir}${path.sep}.dami-harness/env.sh`);
      expect(content).toContain('# my config');
      expect(content).toContain('# other config');
      // Only one start/end pair
      expect(content.split(DAMI_ENV_START).length).toBe(2);
      expect(content.split(DAMI_ENV_END).length).toBe(2);
    });

    it('should create shell profile if it does not exist', async () => {
      vi.stubEnv('SHELL', '/bin/bash');
      const bashrcPath = path.join(homeDir, '.bashrc');
      // Don't create .bashrc — it should be created by installItem

      await handler.installItem(item, teamConfig, localConfig);

      expect(await fse.pathExists(bashrcPath)).toBe(true);
      const content = await fse.readFile(bashrcPath, 'utf-8');
      expect(content).toContain(DAMI_ENV_START);
    });

    it('should skip shell injection when injectShellProfile is false', async () => {
      const noInjectConfig: HarnessConfig = {
        ...teamConfig,
        sharing: {
          ...teamConfig.sharing,
          env: { injectShellProfile: false },
        },
      };

      vi.stubEnv('SHELL', '/bin/bash');
      const bashrcPath = path.join(homeDir, '.bashrc');
      await fse.writeFile(bashrcPath, '# original\n');

      await handler.installItem(item, noInjectConfig, localConfig);

      // Backup and env.sh should still be written
      const backupPath = path.join(homeDir, '.dami-harness', 'env');
      expect(await fse.pathExists(backupPath)).toBe(true);
      const envShPath = path.join(homeDir, '.dami-harness', 'env.sh');
      expect(await fse.pathExists(envShPath)).toBe(true);

      // Shell profile should NOT be modified
      const content = await fse.readFile(bashrcPath, 'utf-8');
      expect(content).toBe('# original\n');
      expect(content).not.toContain(DAMI_ENV_START);
    });

    it('should use custom shellProfilePath when specified', async () => {
      const customPath = path.join(tmpDir, 'custom_profile');
      await fse.writeFile(customPath, '# custom\n');

      const customConfig: HarnessConfig = {
        ...teamConfig,
        sharing: {
          ...teamConfig.sharing,
          env: { injectShellProfile: true, shellProfilePath: customPath },
        },
      };

      await handler.installItem(item, customConfig, localConfig);

      const content = await fse.readFile(customPath, 'utf-8');
      expect(content).toContain(DAMI_ENV_START);
      expect(content).toContain(`[ -f ${homeDir}${path.sep}.dami-harness/env.sh ] && source ${homeDir}${path.sep}.dami-harness/env.sh`);
    });

    it('should skip when env.yaml has no variables', async () => {
      const emptyYamlPath = path.join(repoPath, 'env', 'empty.yaml');
      await fse.writeFile(emptyYamlPath, YAML.stringify({ variables: [] }));

      const emptyItem: ResourceItem = {
        name: 'empty.yaml',
        type: 'env',
        sourcePath: emptyYamlPath,
        relativePath: 'env/empty.yaml',
      };

      vi.stubEnv('SHELL', '/bin/bash');
      const bashrcPath = path.join(homeDir, '.bashrc');
      await fse.writeFile(bashrcPath, '# original\n');

      await handler.installItem(emptyItem, teamConfig, localConfig);

      const content = await fse.readFile(bashrcPath, 'utf-8');
      expect(content).toBe('# original\n');
    });

    it('should handle invalid env.yaml gracefully', async () => {
      const badYamlPath = path.join(repoPath, 'env', 'bad.yaml');
      await fse.writeFile(badYamlPath, ':::bad yaml');

      const badItem: ResourceItem = {
        name: 'bad.yaml',
        type: 'env',
        sourcePath: badYamlPath,
        relativePath: 'env/bad.yaml',
      };

      vi.stubEnv('SHELL', '/bin/bash');
      const bashrcPath = path.join(homeDir, '.bashrc');
      await fse.writeFile(bashrcPath, '# original\n');

      await handler.installItem(badItem, teamConfig, localConfig);

      // Should not crash and should not modify shell profile
      const content = await fse.readFile(bashrcPath, 'utf-8');
      expect(content).toBe('# original\n');
    });
  });

  // ─── removeItem ──────────────────────────────────────────

  describe('removeItem', () => {
    it('should return empty array and warn', async () => {
      const { log } = await import('../utils/logger.js');
      const result = await handler.removeItem('test', teamConfig, localConfig);
      expect(result).toEqual([]);
      expect(log.warn).toHaveBeenCalled();
    });
  });
});
