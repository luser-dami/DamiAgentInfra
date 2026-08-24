import type { LocalConfig, HarnessConfig } from './types.js';

export type UpdatePolicy = 'auto' | 'prompt' | 'skip';

/**
 * Resolve the effective update policy.
 *
 * Priority: local.updatePolicy > team.autoUpdate > 'auto' (default).
 */
export function resolveEffectiveUpdatePolicy(
  localConfig: Pick<LocalConfig, 'updatePolicy'> | null,
  teamConfig: Pick<HarnessConfig, 'autoUpdate'> | null,
): UpdatePolicy {
  if (localConfig?.updatePolicy !== undefined) {
    return localConfig.updatePolicy;
  }
  if (teamConfig?.autoUpdate === false) return 'skip';
  if (teamConfig?.autoUpdate === true) return 'auto';
  return 'auto';
}

/**
 * Return a new LocalConfig with the given updatePolicy applied.
 *
 * Pass `undefined` to clear the field (inherit team default).
 */
export function withUpdatePolicy(
  config: LocalConfig,
  policy: UpdatePolicy | undefined,
): LocalConfig {
  const { updatePolicy: _, ...rest } = config;
  return policy === undefined ? (rest as LocalConfig) : { ...rest, updatePolicy: policy };
}

/**
 * Return a new HarnessConfig with the given autoUpdate applied.
 *
 * Pass `undefined` to clear the field (no team opinion).
 */
export function withAutoUpdate(
  config: HarnessConfig,
  value: boolean | undefined,
): HarnessConfig {
  const { autoUpdate: _, ...rest } = config;
  return value === undefined ? (rest as HarnessConfig) : { ...rest, autoUpdate: value };
}
