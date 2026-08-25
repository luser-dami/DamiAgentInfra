import { afterEach, describe, expect, it } from 'vitest';
import os from 'node:os';
import { homeDir } from '../utils/home.js';

const PREV = { HOME: process.env.HOME, USERPROFILE: process.env.USERPROFILE };

afterEach(() => {
  if (PREV.HOME === undefined) delete process.env.HOME;
  else process.env.HOME = PREV.HOME;
  if (PREV.USERPROFILE === undefined) delete process.env.USERPROFILE;
  else process.env.USERPROFILE = PREV.USERPROFILE;
});

describe('homeDir', () => {
  it('prefers HOME when set', () => {
    process.env.HOME = '/tmp/posix-home';
    process.env.USERPROFILE = 'C:\\Users\\ignored';
    expect(homeDir()).toBe('/tmp/posix-home');
  });

  it('falls back to USERPROFILE when HOME is unset', () => {
    delete process.env.HOME;
    process.env.USERPROFILE = 'C:\\Users\\someone';
    expect(homeDir()).toBe('C:\\Users\\someone');
  });

  it('falls back to os.homedir when both are unset', () => {
    delete process.env.HOME;
    delete process.env.USERPROFILE;
    expect(homeDir()).toBe(os.homedir());
  });
});
