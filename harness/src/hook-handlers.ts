/**
 * Hook Handler Registry — maps event+matcher to concrete handler implementations.
 *
 * Each handler wraps an existing dami-harness subcommand function but accepts
 * pre-parsed STDIN data instead of reading from process.stdin directly. This
 * enables the dispatcher to read STDIN once and fan out to all handlers.
 *
 * In this local-only harness the registry is deliberately small: the only
 * built-in hook handler is the background npm-registry update check. All
 * team-workflow handlers (pull / dashboard / usage tracking / contribute /
 * votes / mr-hint / local-agent) were removed with the team features.
 */

import type { HookHandler } from './hook-dispatch.js';
import type { LocalConfig } from './types.js';

// ─── Public types ───────────────────────────────────────

export interface HandlerRegistration {
  event: string;
  matcher: string;
  handler: HookHandler;
  timeoutMs: number;
  /** Fire-and-forget: run detached so it can't delay host hook completion. */
  background?: boolean;
  /**
   * Kept for registry compatibility: marks handlers that require a git-backed
   * store. The local harness registers none, but the dispatch-time filter
   * (filterHandlersForConfig) still understands the flag.
   */
  gitOnly?: boolean;
}

// ─── Timeout constants ──────────────────────────────────

/** Background (detached) npm-registry update check — not awaited by the host. */
const UPDATE_TIMEOUT_MS = 10_000;

// ─── Handler implementations ────────────────────────────
//
// Each handler is a thin adapter that:
//   1. Receives pre-parsed STDIN (Record<string, unknown>)
//   2. Delegates to the actual subcommand logic
//   3. Returns output string or null
//
// IMPORTANT: These use dynamic imports to keep module loading lazy.
// The dispatcher only loads the modules that actually need to run.

const updateHandler: HookHandler = {
  name: 'update',
  async execute(_stdin, _tool) {
    const { doUpdate } = await import('./update.js');
    await doUpdate();
    return null;
  },
};

// ─── Registry builder ───────────────────────────────────

/**
 * Build the complete handler registry for the hook dispatcher.
 * Returns all handler registrations with their event, matcher, timeout, and implementation.
 */
export function buildHandlerRegistry(): HandlerRegistration[] {
  return [
    // ─── Stop ─────────────────────────────────────────
    // The update check shells out to the npm registry, so it runs detached to
    // avoid pushing the Stop hook past the host's hook timeout (some hosts kill
    // hooks at ~10s regardless of the declared timeout).
    { event: 'stop', matcher: '*', handler: updateHandler, timeoutMs: UPDATE_TIMEOUT_MS, background: true },
  ];
}

/**
 * Apply the provider-config gate to a handler registry.
 *
 * Legacy configs whose store was declared HTTP read-only
 * (localConfig.repo.kind === 'http') must not receive handlers flagged
 * `gitOnly`. When localConfig is null (not initialized) or the kind is
 * anything else, the full registry is returned unchanged.
 *
 * Fail-open by design: a null localConfig means either dami-harness is not
 * initialized or the config failed to parse. In both cases the full registry
 * is kept, so a corrupted config degrades to "all hooks run" rather than
 * silently disabling them.
 */
export function filterHandlersForConfig(
  registry: HandlerRegistration[],
  localConfig: LocalConfig | null,
): HandlerRegistration[] {
  if (localConfig?.repo.kind === 'http') {
    return registry.filter((reg) => reg.gitOnly !== true);
  }
  return registry;
}
