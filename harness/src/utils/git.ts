import path from 'node:path';
import fse from 'fs-extra';
import simpleGit, { type SimpleGit } from 'simple-git';

/**
 * Create a SimpleGit instance for a given base path.
 *
 * Local-only: no remotes are configured or contacted. Used for best-effort
 * dirty-state display when the harness store happens to be a git repo.
 */
export function createGit(basePath?: string): SimpleGit {
  if (basePath) {
    return simpleGit({ baseDir: basePath });
  }
  return simpleGit();
}

/**
 * Check whether localPath is a valid git repository (has a `.git` entry).
 *
 * Returns false if the path does not exist, or exists but is not a git repo.
 * Callers use this to avoid running git commands against a non-repo.
 */
export async function isGitRepo(localPath: string): Promise<boolean> {
  if (!(await fse.pathExists(localPath))) {
    return false;
  }
  return fse.pathExists(path.join(localPath, '.git'));
}

/**
 * Best-effort local dirty-state for the harness store.
 *
 * Purely local: no fetch, no remote contact. `ahead`/`behind` come from
 * simple-git's local tracking info and are 0 when the store has no upstream
 * (the common case for a local-only store).
 */
export async function getRepoStatus(localPath: string): Promise<{ ahead: number; behind: number; modified: string[] }> {
  const git = createGit(localPath);
  const status = await git.status();
  return {
    ahead: status.ahead,
    behind: status.behind,
    modified: [...status.modified, ...status.not_added, ...status.created],
  };
}
