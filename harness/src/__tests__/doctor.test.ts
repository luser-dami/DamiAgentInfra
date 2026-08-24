import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';

// ── Mocks ────────────────────────────────────────────────

vi.mock('../config.js', () => ({
    loadLocalConfig: vi.fn(),
    loadTeamConfig: vi.fn(),
    detectProjectConfig: vi.fn().mockResolvedValue(null),
}));

vi.mock('../utils/fs.js', () => ({
    pathExists: vi.fn(),
    readFileSafe: vi.fn(),
}));

vi.mock('../utils/logger.js', () => ({
    log: {
        info: vi.fn(),
        success: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

// ── Imports (after mocks) ────────────────────────────────

import { loadLocalConfig, loadTeamConfig } from '../config.js';
import { pathExists, readFileSafe } from '../utils/fs.js';
import { DAMI_HOOK_SUBCOMMANDS } from '../hooks.js';
import { doctor } from '../doctor.js';

const mockedLoadLocalConfig = loadLocalConfig as Mock;
const mockedLoadTeamConfig = loadTeamConfig as Mock;
const mockedPathExists = pathExists as Mock;
const mockedReadFileSafe = readFileSafe as Mock;

const mockLocalConfig = {
    repo: { localPath: '/tmp/repo', remote: 'https://git.woa.com/team/repo.git' },
    username: 'testuser',
    updatePolicy: 'auto',
};

const mockTeamConfig = {
    team: 'test-team',
    repo: 'team/repo',
    provider: 'tgit' as const,
    toolPaths: {
        claude: { settings: '.claude/settings.json', skills: '.claude/skills' },
    },
};

// Build a settings content that contains all subcommands
function buildFullHooksContent(): string {
    const lines = DAMI_HOOK_SUBCOMMANDS.map(
        (sub) => `"command": "bash -lc \\"dami-harness ${sub}\\""`,
    );
    return `{ "hooks": { ${lines.join(', ')} } }`;
}

// Build a settings content that is missing some subcommands
function buildPartialHooksContent(exclude: string[]): string {
    const subs = DAMI_HOOK_SUBCOMMANDS.filter((s) => !exclude.includes(s));
    const lines = subs.map(
        (sub) => `"command": "bash -lc \\"dami-harness ${sub}\\""`,
    );
    return `{ "hooks": { ${lines.join(', ')} } }`;
}

// ── Setup ────────────────────────────────────────────────

// doctor routes human output through log.info (mocked above) so stdout stays
// a clean JSON channel under --json.
import { log } from '../utils/logger.js';
const logInfoSpy = log.info as Mock;

beforeEach(() => {
    vi.clearAllMocks();
    mockedLoadLocalConfig.mockResolvedValue(mockLocalConfig);
    mockedLoadTeamConfig.mockResolvedValue(mockTeamConfig);
    mockedPathExists.mockResolvedValue(true);
    mockedReadFileSafe.mockResolvedValue(buildFullHooksContent());
});

// ── Tests ────────────────────────────────────────────────

describe('doctor — hook checks', () => {
    it('should pass when all subcommands are present in settings', async () => {
        await doctor({});

        // Should show the hooks check passing (✔)
        expect(logInfoSpy).toHaveBeenCalledWith(
            expect.stringContaining('✔'),
        );
    });

    it('should fail when a subcommand is missing from settings', async () => {
        // Missing 'hook-dispatch' subcommand (the only required one now)
        mockedReadFileSafe.mockImplementation(async (filePath: string) => {
            if (filePath.includes('settings.json')) {
                // Return settings without hook-dispatch
                return '{ "hooks": { "command": "bash -lc \\"dami-harness pull\\"" } }';
            }
            if (filePath.includes('.zshrc') || filePath.includes('.bashrc')) {
                return '# [dami-harness:env:start]';
            }
            return null;
        });

        await doctor({});

        // Should show the hooks check failing (✖) with fix suggestion
        expect(logInfoSpy).toHaveBeenCalledWith(
            expect.stringContaining('✖'),
        );
        expect(logInfoSpy).toHaveBeenCalledWith(
            expect.stringContaining('dami-harness hooks inject'),
        );
    });

    it('should fail when settings file does not exist', async () => {
        mockedPathExists.mockImplementation(async (filePath: string) => {
            if (filePath.includes('settings.json')) return false;
            return true;
        });

        await doctor({});

        // Should show at least one failing check
        expect(logInfoSpy).toHaveBeenCalledWith(
            expect.stringContaining('✖'),
        );
    });

    it('should check all DAMI_HOOK_SUBCOMMANDS', () => {
        // With the merged dispatch format, only hook-dispatch is needed
        expect(DAMI_HOOK_SUBCOMMANDS).toContain('hook-dispatch');
        expect(DAMI_HOOK_SUBCOMMANDS).toHaveLength(1);
    });

    it('should pass env check when env/env.yaml does not exist in team repo', async () => {
        mockedPathExists.mockImplementation(async (filePath: string) => {
            // path.join uses backslashes on Windows — normalize before matching.
            if (filePath.replaceAll('\\', '/').includes('env/env.yaml')) return false;
            return true;
        });
        mockedReadFileSafe.mockImplementation(async (filePath: string) => {
            if (filePath.includes('settings.json')) return buildFullHooksContent();
            return null;
        });

        await doctor({});

        const allCalls = logInfoSpy.mock.calls.map((c) => c[0]);
        const envLine = allCalls.find((msg: string) => msg.includes('Env variables'));
        expect(envLine).toContain('✔');
    });

    it('should pass env check when injectShellProfile is false', async () => {
        mockedLoadTeamConfig.mockResolvedValue({
            ...mockTeamConfig,
            sharing: { env: { injectShellProfile: false } },
        });
        mockedPathExists.mockImplementation(async (filePath: string) => {
            if (filePath.includes('env.sh')) return false;
            return true;
        });
        mockedReadFileSafe.mockImplementation(async (filePath: string) => {
            if (filePath.includes('settings.json')) return buildFullHooksContent();
            return null;
        });

        await doctor({});

        const allCalls = logInfoSpy.mock.calls.map((c) => c[0]);
        const envLine = allCalls.find((msg: string) => msg.includes('Env variables'));
        expect(envLine).toContain('✔');
    });

    it('should skip tools whose parent directory does not exist', async () => {
        mockedLoadTeamConfig.mockResolvedValue({
            ...mockTeamConfig,
            toolPaths: {
                claude: { settings: '.claude/settings.json', skills: '.claude/skills' },
                'codex-internal': { settings: '.codex-internal/hooks.json', skills: '.codex-internal/skills' },
            },
        });

        mockedPathExists.mockImplementation(async (filePath: string) => {
            // .codex-internal directory does not exist
            if (filePath.includes('.codex-internal')) return false;
            return true;
        });

        await doctor({});

        // Should NOT show codex-internal check at all (skipped)
        const allCalls = logInfoSpy.mock.calls.map((c) => c[0]);
        expect(allCalls.some((msg: string) => msg.includes('codex-internal'))).toBe(false);
        // Should still show claude check
        expect(allCalls.some((msg: string) => msg.includes('claude'))).toBe(true);
    });
});
