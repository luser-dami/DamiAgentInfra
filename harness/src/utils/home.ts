import os from 'node:os';

/**
 * Canonical user-home resolution. `process.env.HOME` is POSIX-only and may be
 * unset on Windows (where USERPROFILE is the standard), and shells may clear
 * it entirely — never read process.env.HOME directly.
 */
export function homeDir(): string {
  return process.env.HOME || process.env.USERPROFILE || os.homedir();
}
