import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import path from 'node:path';
import os from 'node:os';
import fse from 'fs-extra';
import { fileURLToPath } from 'node:url';

vi.mock('../utils/logger.js', () => ({
  log: { info: vi.fn(), success: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { reconcileHooksToAllTools } from '../hooks.js';

// Verify that reconcileHooksToAllTools (the pull/init main path) creates the
// dami-harness wrapper at $HOME/.dami-harness/bin/dami-harness when workbuddy or codebuddy is present.
//
// resolveHarnessEntryScript() looks for cli.js next to builtin-hooks.ts (src/).
// We create a stub src/cli.js so resolution succeeds in the test environment.

const srcDir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const stubIndexJs = path.join(srcDir, 'cli.js');

describe('reconcileHooksToAllTools — wrapper creation on main inject path', () => {
  let tmp: string;
  let origHome: string | undefined;
  let stubCreated = false;

  beforeEach(async () => {
    tmp = await fse.mkdtemp(path.join(os.tmpdir(), 'hooks-wrapper-'));
    origHome = process.env.HOME;
    process.env.HOME = tmp;

    // Create stub cli.js so resolveHarnessEntryScript() succeeds in test env
    if (!await fse.pathExists(stubIndexJs)) {
      await fse.writeFile(stubIndexJs, '// test stub\n');
      stubCreated = true;
    }
  });

  afterEach(async () => {
    if (origHome !== undefined) process.env.HOME = origHome;
    else delete process.env.HOME;
    await fse.remove(tmp);
    if (stubCreated) {
      await fse.remove(stubIndexJs);
      stubCreated = false;
    }
  });

  for (const tool of ['workbuddy', 'codebuddy']) {
    it.skipIf(process.platform === 'win32')(`creates wrapper when ${tool} is in toolPaths`, async () => {
      const settingsFile = `.${tool}/settings.json`;
      const toolRoot = path.join(tmp, `.${tool}`);
      await fse.ensureDir(toolRoot);

      const toolPaths: Record<string, { settings?: string }> = {
        [tool]: { settings: settingsFile },
      };

      await reconcileHooksToAllTools(toolPaths, tmp, [], path.join(tmp, 'managed-hooks.json'), {});

      const wrapperPath = path.join(tmp, '.dami-harness', 'bin', 'dami-harness');
      expect(await fse.pathExists(wrapperPath)).toBe(true);

      const wrapperContent = await fse.readFile(wrapperPath, 'utf-8');
      expect(wrapperContent).toContain('exec');
      expect(wrapperContent).toContain('cli.js');
    });
  }

  it('does not throw when neither workbuddy nor codebuddy is present', async () => {
    const toolPaths: Record<string, { settings?: string }> = {
      claude: { settings: '.claude/settings.json' },
    };
    await fse.ensureDir(path.join(tmp, '.claude'));

    await expect(
      reconcileHooksToAllTools(toolPaths, tmp, [], path.join(tmp, 'managed-hooks.json'), {}),
    ).resolves.not.toThrow();
  });
});

describe('reconcileHooksToAllTools — wrapper creation on main inject path', () => {
  let tmp: string;
  let origHome: string | undefined;
  let stubCreated = false;

  beforeEach(async () => {
    tmp = await fse.mkdtemp(path.join(os.tmpdir(), 'hooks-wrapper-'));
    origHome = process.env.HOME;
    process.env.HOME = tmp;

    // Create stub cli.js so resolveHarnessEntryScript() succeeds in test env
    if (!await fse.pathExists(stubIndexJs)) {
      await fse.writeFile(stubIndexJs, '// test stub\n');
      stubCreated = true;
    }
  });

  afterEach(async () => {
    if (origHome !== undefined) process.env.HOME = origHome;
    else delete process.env.HOME;
    await fse.remove(tmp);
    if (stubCreated) {
      await fse.remove(stubIndexJs);
      stubCreated = false;
    }
  });

  for (const tool of ['workbuddy', 'codebuddy']) {
    it.skipIf(process.platform === 'win32')(`creates wrapper when ${tool} is in toolPaths`, async () => {
      const settingsFile = `.${tool}/settings.json`;
      const toolRoot = path.join(tmp, `.${tool}`);
      await fse.ensureDir(toolRoot);

      const toolPaths: Record<string, { settings?: string }> = {
        [tool]: { settings: settingsFile },
      };

      await reconcileHooksToAllTools(toolPaths, tmp, [], path.join(tmp, 'managed-hooks.json'), {});

      const wrapperPath = path.join(tmp, '.dami-harness', 'bin', 'dami-harness');
      expect(await fse.pathExists(wrapperPath)).toBe(true);

      const wrapperContent = await fse.readFile(wrapperPath, 'utf-8');
      expect(wrapperContent).toContain('exec');
      expect(wrapperContent).toContain('cli.js');
    });
  }

  it('does not throw when neither workbuddy nor codebuddy is present', async () => {
    const toolPaths: Record<string, { settings?: string }> = {
      claude: { settings: '.claude/settings.json' },
    };
    await fse.ensureDir(path.join(tmp, '.claude'));

    await expect(
      reconcileHooksToAllTools(toolPaths, tmp, [], path.join(tmp, 'managed-hooks.json'), {}),
    ).resolves.not.toThrow();
  });
});
