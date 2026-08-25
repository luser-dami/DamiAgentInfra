import { homeDir } from '../utils/home.js';
import path from 'node:path';
import type { ResourceType, ResourceItem, ResourceDiff, HarnessConfig, LocalConfig } from '../types.js';
import { readFileSafe, writeFile, ensureDir, pathExists } from '../utils/fs.js';

const TOMBSTONE_FILE = '.removed';

/**
 * Abstract base class for resource handlers.
 * Each resource type (skills, rules, docs, env, agents, hooks, mcp) implements this.
 */
export abstract class ResourceHandler {
  abstract readonly type: ResourceType;

  /**
   * Scan local agent tool directories for items that could be collected into
   * the harness store. Returns items found locally that are not yet in the store.
   */
  abstract scanLocalForCollect(
    teamConfig: HarnessConfig,
    localConfig: LocalConfig,
  ): Promise<ResourceItem[]>;

  /**
   * Scan the harness store for items that should be installed locally.
   * Returns items from the store.
   */
  abstract scanStoreForInstall(
    teamConfig: HarnessConfig,
    localConfig: LocalConfig,
  ): Promise<ResourceItem[]>;

  /**
   * Collect a resource item from a local tool directory into the store.
   */
  abstract collectItem(
    item: ResourceItem,
    teamConfig: HarnessConfig,
    localConfig: LocalConfig,
  ): Promise<void>;

  /**
   * Install a resource item from the store into local AI tool directories.
   */
  abstract installItem(
    item: ResourceItem,
    teamConfig: HarnessConfig,
    localConfig: LocalConfig,
  ): Promise<void>;

  /**
   * Remove a resource from the store and all local AI tool directories.
   * Returns the list of paths that were removed.
   */
  abstract removeItem(
    name: string,
    teamConfig: HarnessConfig,
    localConfig: LocalConfig,
  ): Promise<string[]>;

  /**
   * Check if an AI tool is installed by verifying its root directory exists.
   * e.g. for toolPath ".claude/skills", checks if ~/.claude/ exists.
   * This prevents creating directories for tools the user hasn't installed.
   * @param baseDir - Override base directory (defaults to HOME). Used for project scope.
   */
  static async isToolInstalled(toolPath: string, baseDir?: string): Promise<boolean> {
    const base = baseDir ?? homeDir() ?? '';
    const toolRoot = path.join(base, toolPath.split('/')[0]);
    return pathExists(toolRoot);
  }

  /**
   * Read the tombstone file (`<type>/.removed`) from the store.
   * Returns a Set of resource names that have been explicitly deleted.
   */
  async readTombstones(localConfig: LocalConfig): Promise<Set<string>> {
    const tombstonePath = path.join(localConfig.repo.localPath, this.type, TOMBSTONE_FILE);
    const content = await readFileSafe(tombstonePath);
    if (!content) return new Set();
    return new Set(
      content.split('\n').map((l) => l.trim()).filter((l) => l.length > 0),
    );
  }

  /**
   * Append a resource name to the tombstone file, deduplicating and sorting.
   */
  async addTombstone(name: string, localConfig: LocalConfig): Promise<void> {
    const dir = path.join(localConfig.repo.localPath, this.type);
    await ensureDir(dir);
    const tombstonePath = path.join(dir, TOMBSTONE_FILE);
    const existing = await this.readTombstones(localConfig);
    existing.add(name);
    const sorted = [...existing].sort();
    await writeFile(tombstonePath, sorted.join('\n') + '\n');
  }

  /**
   * Compute diff between local tool directories and the store for this resource type.
   */
  async diff(
    teamConfig: HarnessConfig,
    localConfig: LocalConfig,
  ): Promise<ResourceDiff> {
    const localItems = await this.scanLocalForCollect(teamConfig, localConfig);
    const teamItems = await this.scanStoreForInstall(teamConfig, localConfig);

    const teamNames = new Set(teamItems.map((i) => i.name));
    const localNames = new Set(localItems.map((i) => i.name));

    const added = localItems.filter((i) => !teamNames.has(i.name));
    const removed = teamItems.filter((i) => !localNames.has(i.name));

    return { added, modified: [], removed };
  }
}
