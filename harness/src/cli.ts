import { createRequire } from 'node:module';
import { Command } from 'commander';
import { setVerbose, setSilent, setStderrOnly, log } from './utils/logger.js';
import type { GlobalOptions, ResourceType } from './types.js';

const require = createRequire(import.meta.url);
const { version } = require('../package.json');

const SUMMARY =
  'Local harness resource manager for AI coding agents: a versioned local store ' +
  'of skills/rules/docs/agents/hooks/mcp resources, scanned and injected into ' +
  'detected agent tool directories.';

// ─── Tool contract plumbing ─────────────────────────────
//
//  --json: machine JSON on stdout, human output on stderr.
//  Exit codes: 0 success, 2 usage error, 3 environment error, 4 domain error
//  (e.g. not initialized), 1 unexpected failure.

interface VerbSpec {
  name: string;
  summary: string;
  args: Record<string, unknown>;
}

const VERBS: VerbSpec[] = [
  {
    name: 'init',
    summary: 'Initialize dami-harness: create the local store, write config, inject hooks into detected agent tools',
    args: {
      type: 'object',
      properties: {
        scope: { type: 'string', enum: ['user', 'project'], description: 'Install scope (default: project, or user when cwd is $HOME)' },
        agent: { type: 'array', items: { type: 'string' }, description: 'AI tools to set up (repeatable or comma-separated)' },
        force: { type: 'boolean', description: 'Overwrite existing config without confirmation' },
        'inherit-user-scope': { type: 'boolean', description: 'In project scope, also sync safe user-scope resources' },
      },
    },
  },
  {
    name: 'status',
    summary: 'Show store status, sync state and resource counts',
    args: { type: 'object', properties: {} },
  },
  {
    name: 'list',
    summary: 'List resources (skills|rules|docs|agents|hooks|mcp) in the store and/or installed agent directories',
    args: {
      type: 'object',
      properties: {
        type: { type: 'string', enum: ['skills', 'rules', 'docs', 'agents', 'hooks', 'mcp'] },
        source: { type: 'string', enum: ['repo', 'local', 'all'], description: 'Where to look (default: all)' },
        agent: { type: 'string', description: 'Filter local agents by id (skills only)' },
      },
    },
  },
  {
    name: 'skill show',
    summary: 'Show skill metadata: source / installed agents / description',
    args: { type: 'object', properties: { name: { type: 'string' } }, required: ['name'] },
  },
  {
    name: 'skill exclude',
    summary: 'Manage per-user skill exclusion: skill exclude [list] | add <skills...> | remove <skills...>',
    args: {
      type: 'object',
      properties: { skills: { type: 'array', items: { type: 'string' } } },
    },
  },
  {
    name: 'hooks list',
    summary: 'List hook install status and effective built-in and store hooks',
    args: { type: 'object', properties: {} },
  },
  {
    name: 'hooks inject',
    summary: 'Inject dami-harness hooks into all detected AI tool settings',
    args: { type: 'object', properties: { silent: { type: 'boolean' } } },
  },
  {
    name: 'hooks remove',
    summary: 'Remove dami-harness hooks from all AI tool settings',
    args: { type: 'object', properties: {} },
  },
  {
    name: 'mcp list',
    summary: 'List store MCP servers and their per-tool install status',
    args: { type: 'object', properties: {} },
  },
  {
    name: 'mcp inject',
    summary: 'Inject store MCP servers into all AI tool configs',
    args: {
      type: 'object',
      properties: {
        'dry-run': { type: 'boolean' },
        force: { type: 'boolean', description: 'Overwrite servers that collide with user-owned entries' },
      },
    },
  },
  {
    name: 'mcp remove',
    summary: 'Remove all dami-harness-managed MCP servers from AI tool configs',
    args: { type: 'object', properties: {} },
  },
  {
    name: 'doctor',
    summary: 'Diagnose configuration and installation issues',
    args: { type: 'object', properties: {} },
  },
  {
    name: 'uninstall',
    summary: 'Remove all dami-harness-managed resources and hooks from this machine',
    args: {
      type: 'object',
      properties: {
        force: { type: 'boolean', description: 'Skip confirmation prompt' },
        agent: { type: 'string', description: 'Only uninstall this agent\'s resources' },
      },
    },
  },
];

function printDescribe(): void {
  process.stdout.write(JSON.stringify({
    name: 'dami-harness',
    version,
    summary: SUMMARY,
    contract: 1,
    verbs: VERBS,
  }) + '\n');
}

/** Classify an error into the contract exit code. */
function exitCodeFor(err: Error): number {
  if (/not initialized/.test(err.message)) return 4;
  return 1;
}

/** Run a command action under the tool contract: errors → stderr + exit code. */
async function run(fn: () => Promise<void>): Promise<void> {
  try {
    await fn();
  } catch (e) {
    const err = e as Error;
    log.error(err.message);
    process.exit(exitCodeFor(err));
  }
}

/** Emit a machine-readable result envelope for action verbs in --json mode. */
function emitJson(options: GlobalOptions, data: Record<string, unknown>): void {
  if (options.json) {
    process.stdout.write(JSON.stringify({ ok: true, ...data }) + '\n');
  }
}

// ─── Program ────────────────────────────────────────────

const program = new Command();

program
  .name('dami-harness')
  .description(SUMMARY)
  .version(version)
  .option('--json', 'Machine-readable JSON output on stdout; human output on stderr')
  .option('--dry-run', 'Preview mode, no changes made')
  .option('-v, --verbose', 'Verbose output')
  .hook('preAction', (thisCommand) => {
    const opts = program.opts();
    if (opts.verbose) setVerbose(true);
    if (opts.json) setStderrOnly(true);
  });

// Usage/argument errors are contract exit code 2 (commander defaults to 1).
program.exitOverride((err) => {
  process.exit(err.exitCode === 0 ? 0 : 2);
});

if (process.argv.includes('--describe')) {
  printDescribe();
  process.exit(0);
}

program
  .command('init')
  .description('Initialize dami-harness (local store + config + hook injection)')
  .option('--scope <scope>', 'Install scope: project (default, <cwd>/.dami-harness + <cwd>/.claude) or user (~/.dami-harness + ~/.claude)')
  .option('--inherit-user-scope', 'In project scope, also sync safe user-scope resources')
  .option('--no-inherit-user-scope', 'Disable user-scope inheritance for this project')
  // Non-variadic + a collecting coercer: repeatable (`--agent a --agent b`) and
  // comma-separated (`--agent a,b`, split later by normalizeAgentList) both work.
  .option('--agent <name>', 'AI tools to set up (e.g. claude, codex, cursor, omp). Repeatable or comma-separated. Additive on repeated runs.', (val: string, acc: string[]) => acc.concat(val), [] as string[])
  .option('--force', 'Overwrite existing config without confirmation')
  .action(async (cmdOpts) => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { init } = await import('./init.js');
      await init({ ...globalOpts, ...cmdOpts });
      emitJson(globalOpts, { scope: cmdOpts.scope ?? 'auto' });
    });
  });

program
  .command('status')
  .description('Show store status, sync state and resource counts')
  .action(async () => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      if (globalOpts.json) {
        const { statusJson } = await import('./status.js');
        process.stdout.write(JSON.stringify(await statusJson()) + '\n');
        return;
      }
      const { status } = await import('./status.js');
      await status(globalOpts);
    });
  });

program
  .command('list [type]')
  .description('List resources (skills|rules|docs|agents|hooks|mcp). For skills, --source local/all also scans installed AI agent skill directories.')
  .option('--source <src>', 'Where to look for skills: repo | local | all', 'all')
  .option('--agent <name>', 'Filter local agents by id (only applies to skills)')
  .action(async (type, cmdOpts) => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      if (globalOpts.json) {
        const { listJson } = await import('./status.js');
        process.stdout.write(JSON.stringify(await listJson(type as ResourceType | undefined)) + '\n');
        return;
      }
      const { list } = await import('./status.js');
      await list(type, { ...globalOpts, ...cmdOpts });
    });
  });

const skillCmd = program
  .command('skill')
  .description('List and inspect skills (default: list all skills across store + installed agents)')
  .action(async () => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { list } = await import('./status.js');
      await list('skills', { ...globalOpts, source: 'all' });
    });
  });

skillCmd
  .command('list')
  .description('List all skills (alias for: dami-harness list skills --source all)')
  .action(async () => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { list } = await import('./status.js');
      await list('skills', { ...globalOpts, source: 'all' });
    });
  });

skillCmd
  .command('show <name>')
  .description('Show skill metadata: source / installed agents / description')
  .action(async (name: string, cmdOpts) => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { skillShow } = await import('./skill-cmd.js');
      await skillShow(name, { ...globalOpts, ...cmdOpts });
    });
  });

const excludeCmd = skillCmd
  .command('exclude')
  .description('Manage per-user skill exclusion (skip sync without affecting the store)')
  .action(async () => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { excludeList } = await import('./exclude.js');
      await excludeList(globalOpts);
    });
  });

excludeCmd
  .command('list')
  .description('List excluded skills')
  .action(async () => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { excludeList } = await import('./exclude.js');
      await excludeList(globalOpts);
    });
  });

excludeCmd
  .command('add <skills...>')
  .description('Add skill(s) to the exclude list')
  .action(async (skills: string[]) => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { excludeAdd } = await import('./exclude.js');
      await excludeAdd(skills, globalOpts);
      emitJson(globalOpts, { excluded: skills });
    });
  });

excludeCmd
  .command('remove <skills...>')
  .description('Remove skill(s) from the exclude list')
  .action(async (skills: string[]) => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { excludeRemove } = await import('./exclude.js');
      await excludeRemove(skills, globalOpts);
      emitJson(globalOpts, { included: skills });
    });
  });

program
  .command('doctor')
  .description('Diagnose configuration and installation issues')
  .action(async () => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { doctor } = await import('./doctor.js');
      const results = await doctor(globalOpts);
      if (globalOpts.json) {
        process.stdout.write(JSON.stringify({
          ok: results.every((r) => r.ok),
          checks: results,
        }) + '\n');
      }
      if (results.some((r) => !r.ok)) process.exit(3);
    });
  });

program
  .command('uninstall')
  .description('Remove all dami-harness-managed resources and hooks from this machine')
  .option('--force', 'Skip confirmation prompt')
  .option('--agent <name>', 'Only uninstall this agent\'s resources; shared resources go only if it is the last tool')
  .action(async (cmdOpts) => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { uninstall } = await import('./uninstall.js');
      await uninstall({ ...globalOpts, ...cmdOpts });
      emitJson(globalOpts, {});
    });
  });

// ─── Hooks subcommand ───────────────────────────────────

const hooksCmd = program
  .command('hooks')
  .description('Manage dami-harness hooks in AI tool settings');

hooksCmd
  .command('list')
  .description('List hook install status + effective built-in (A) and store (B) hooks')
  .action(async () => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { hooksList } = await import('./hooks-cmd.js');
      await hooksList(globalOpts);
    });
  });

hooksCmd
  .command('inject')
  .description('Inject dami-harness hooks into all AI tool settings')
  .option('--silent', 'Silent mode (suppress success message)')
  .action(async (cmdOpts) => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      if (cmdOpts.silent) setSilent(true);
      const { hooksInject } = await import('./hooks-cmd.js');
      await hooksInject({ ...globalOpts, ...cmdOpts });
      emitJson(globalOpts, {});
    });
  });

hooksCmd
  .command('remove')
  .description('Remove dami-harness hooks from all AI tool settings')
  .action(async () => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { hooksRemove } = await import('./hooks-cmd.js');
      await hooksRemove(globalOpts);
      emitJson(globalOpts, {});
    });
  });

// ─── MCP subcommand ─────────────────────────────────────

const mcpCmd = program
  .command('mcp')
  .description('Manage store MCP servers across AI tools');

mcpCmd
  .command('list')
  .description('List store MCP servers and their per-tool install status')
  .action(async () => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { mcpList } = await import('./mcp-cmd.js');
      await mcpList(globalOpts);
    });
  });

mcpCmd
  .command('inject')
  .description('Inject store MCP servers into all AI tool configs')
  .option('--dry-run', 'Show what would change without writing')
  .option('--force', 'Overwrite servers that collide with user-owned entries')
  .action(async (cmdOpts) => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { mcpInject } = await import('./mcp-cmd.js');
      await mcpInject({ ...globalOpts, ...cmdOpts });
      emitJson(globalOpts, {});
    });
  });

mcpCmd
  .command('remove')
  .description('Remove all dami-harness-managed MCP servers from AI tool configs')
  .action(async () => {
    const globalOpts = program.opts() as GlobalOptions;
    await run(async () => {
      const { mcpRemove } = await import('./mcp-cmd.js');
      await mcpRemove(globalOpts);
      emitJson(globalOpts, {});
    });
  });

// ─── Hook dispatch (hidden; used by IDE hook subprocesses) ──

program
  .command('hook-dispatch <event>', { hidden: true })
  .description('Unified hook dispatcher — handles all dami-harness hooks for a given event in one process')
  .option('--stdin', 'Read hook data from STDIN (accepted for forward compat, always reads STDIN)')
  .option('--tool <name>', 'Tool identifier (e.g. claude, codex, cursor)')
  .option('--matcher <matcher>', 'Hook matcher for PostToolUse (e.g. Skill, Bash)')
  .option('--bg-only', 'Internal: run only fire-and-forget background handlers (used by the detached child)')
  .action(async (event: string, cmdOpts: { stdin?: boolean; tool?: string; matcher?: string; bgOnly?: boolean }) => {
    const bgOnly = cmdOpts.bgOnly ?? false;

    // Hard wall-clock safety net for the FOREGROUND (parent) hook process, which
    // blocks the host IDE's hook. The host aborts a hook at ~10s regardless of
    // any larger declared timeout. Guarantee we exit well before that no matter
    // which stage stalls. The detached `--bg-only` child is unref'd and not
    // awaited by the host, so it is exempt.
    const HOOK_HARD_EXIT_MS = 7_000;
    let hardExit: NodeJS.Timeout | undefined;
    if (!bgOnly) {
      hardExit = setTimeout(() => process.exit(0), HOOK_HARD_EXIT_MS);
      hardExit.unref();
    }

    const { hookDispatchCli } = await import('./hook-dispatch-cli.js');
    try {
      await hookDispatchCli(event, cmdOpts.tool ?? 'claude', cmdOpts.matcher ?? '*', bgOnly);
    } finally {
      if (hardExit) clearTimeout(hardExit);
      // Hook subprocesses must exit promptly: a hung/unreachable backend fetch can
      // leave a socket pending on the event loop, blocking natural exit and
      // tripping the host IDE's default hook timeout. Force exit once dispatch
      // has settled.
      process.exit(0);
    }
  });

await program.parseAsync(process.argv);
