import { describe, it, expect } from 'vitest';
import {
  resolveEffectiveUpdatePolicy,
  withUpdatePolicy,
  withAutoUpdate,
} from '../update-policy.js';
import { HarnessConfigSchema, LocalConfigSchema } from '../types.js';
import type { LocalConfig, HarnessConfig } from '../types.js';

describe('resolveEffectiveUpdatePolicy', () => {
  it('nothing set → auto (legacy default)', () => {
    expect(resolveEffectiveUpdatePolicy(
      { updatePolicy: undefined },
      { autoUpdate: undefined },
    )).toBe('auto');
  });

  it('team.autoUpdate=false → skip', () => {
    expect(resolveEffectiveUpdatePolicy(
      { updatePolicy: undefined },
      { autoUpdate: false },
    )).toBe('skip');
  });

  it('team.autoUpdate=true → auto', () => {
    expect(resolveEffectiveUpdatePolicy(
      { updatePolicy: undefined },
      { autoUpdate: true },
    )).toBe('auto');
  });

  it('local=prompt beats team=false', () => {
    expect(resolveEffectiveUpdatePolicy(
      { updatePolicy: 'prompt' },
      { autoUpdate: false },
    )).toBe('prompt');
  });

  it('local=auto beats team=false', () => {
    expect(resolveEffectiveUpdatePolicy(
      { updatePolicy: 'auto' },
      { autoUpdate: false },
    )).toBe('auto');
  });

  it('local=skip beats team=true', () => {
    expect(resolveEffectiveUpdatePolicy(
      { updatePolicy: 'skip' },
      { autoUpdate: true },
    )).toBe('skip');
  });

  it('null local config falls through to team', () => {
    expect(resolveEffectiveUpdatePolicy(null, { autoUpdate: false })).toBe('skip');
    expect(resolveEffectiveUpdatePolicy(null, null)).toBe('auto');
  });

  it('null team config falls through to default', () => {
    expect(resolveEffectiveUpdatePolicy({ updatePolicy: 'skip' }, null)).toBe('skip');
  });
});

describe('withUpdatePolicy', () => {
  const base: LocalConfig = LocalConfigSchema.parse({
    repo: { localPath: '/tmp/repo', remote: 'https://example.com' },
    username: 'test',
  });

  it('sets updatePolicy without mutating original', () => {
    const result = withUpdatePolicy(base, 'skip');
    expect(result.updatePolicy).toBe('skip');
    expect(base.updatePolicy).toBeUndefined();
  });

  it('clears updatePolicy when passed undefined', () => {
    const withPolicy = { ...base, updatePolicy: 'auto' as const };
    const result = withUpdatePolicy(withPolicy, undefined);
    expect(result.updatePolicy).toBeUndefined();
  });
});

describe('withAutoUpdate', () => {
  const base: HarnessConfig = HarnessConfigSchema.parse({
    team: 'test',
    repo: 'https://example.com',
  });

  it('sets autoUpdate without mutating original', () => {
    const result = withAutoUpdate(base, false);
    expect(result.autoUpdate).toBe(false);
    expect(base.autoUpdate).toBeUndefined();
  });

  it('clears autoUpdate when passed undefined', () => {
    const withFlag = { ...base, autoUpdate: true };
    const result = withAutoUpdate(withFlag, undefined);
    expect(result.autoUpdate).toBeUndefined();
  });
});
