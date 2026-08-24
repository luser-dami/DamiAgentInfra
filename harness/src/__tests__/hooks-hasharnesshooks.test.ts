import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { hasHarnessHooks } from '../hooks.js';

let tmpDir: string;

beforeEach(() => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'dami-harness-hasdami-harnesshooks-test-'));
});

afterEach(() => {
  fs.rmSync(tmpDir, { recursive: true, force: true });
});

describe('hasHarnessHooks', () => {
  it('returns true when file contains a dami-harness built-in hook (description prefix)', async () => {
    const settingsPath = path.join(tmpDir, 'settings.json');
    fs.writeFileSync(settingsPath, JSON.stringify({
      hooks: {
        SessionStart: [
          {
            matcher: '*',
            hooks: [{ type: 'command', command: 'dami-harness hook-dispatch' }],
            description: '[dami-harness] hook-dispatch',
          },
        ],
      },
    }));

    expect(await hasHarnessHooks(settingsPath, 'claude')).toBe(true);
  });

  it('returns false when file has only user hooks and manifest has stale record (P2#2 regression)', async () => {
    const settingsPath = path.join(tmpDir, 'settings.json');
    fs.writeFileSync(settingsPath, JSON.stringify({
      hooks: {
        SessionStart: [
          { matcher: '*', hooks: [{ type: 'command', command: 'echo hi' }] },
        ],
      },
    }));

    const manifestPath = path.join(tmpDir, 'managed-hooks.json');
    fs.writeFileSync(manifestPath, JSON.stringify({
      claude: [{ id: 'x', event: 'SessionStart', command: 'dami-harness pull' }],
    }));

    // stale manifest entry — command not present in file → must return false
    expect(await hasHarnessHooks(settingsPath, 'claude', manifestPath)).toBe(false);
  });

  it('returns true for codex format when manifest command exists in hooks file', async () => {
    const hooksPath = path.join(tmpDir, 'hooks.json');
    const teamCmd = 'dami-harness-custom-team-hook';
    fs.writeFileSync(hooksPath, JSON.stringify({
      hooks: {
        SessionStart: [
          { hooks: [{ type: 'command', command: teamCmd }] },
        ],
      },
    }));

    const manifestPath = path.join(tmpDir, 'managed-hooks.json');
    fs.writeFileSync(manifestPath, JSON.stringify({
      codex: [{ id: 'y', event: 'SessionStart', command: teamCmd }],
    }));

    expect(await hasHarnessHooks(hooksPath, 'codex', manifestPath)).toBe(true);
  });
});
