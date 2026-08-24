import path from 'node:path';
import fse from 'fs-extra';
import { ResourceHandler } from './base.js';
import type { ResourceItem, HarnessConfig, LocalConfig } from '../types.js';
import { resolveBaseDir } from '../types.js';
import { pathExists, expandHome, listFiles } from '../utils/fs.js';
import { log } from '../utils/logger.js';

export class DocsHandler extends ResourceHandler {
  readonly type = 'docs' as const;

  async scanLocalForCollect(_teamConfig: HarnessConfig, _localConfig: LocalConfig): Promise<ResourceItem[]> {
    // Docs are managed directly in team repo
    return [];
  }

  async scanStoreForInstall(_teamConfig: HarnessConfig, localConfig: LocalConfig): Promise<ResourceItem[]> {
    const docsDir = path.join(localConfig.repo.localPath, 'docs');
    if (!await pathExists(docsDir)) return [];

    // Check if there are actual doc files (ignore .gitkeep and other dot files)
    const files = await listFiles(docsDir);
    const realFiles = files.filter(f => !f.startsWith('.'));
    if (realFiles.length === 0) return [];

    return [{
      name: 'docs',
      type: 'docs',
      sourcePath: docsDir,
      relativePath: 'docs/',
    }];
  }

  async countDocFiles(sourcePath: string): Promise<number> {
    const files = await listFiles(sourcePath);
    return files.filter(f => !f.startsWith('.')).length;
  }

  async collectItem(_item: ResourceItem, _teamConfig: HarnessConfig, _localConfig: LocalConfig): Promise<void> {
    // No-op
  }

  /**
   * Sync docs from team repo to local docs directory.
   */
  async installItem(item: ResourceItem, teamConfig: HarnessConfig, localConfig: LocalConfig): Promise<void> {
    // For project scope, resolve docs dir relative to projectRoot
    const docsLocalDir = teamConfig.sharing.docs.localDir;
    let localDocsDir: string;
    if (localConfig.scope === 'project' && localConfig.projectRoot) {
      // Replace ~ with projectRoot
      localDocsDir = docsLocalDir.startsWith('~/')
        ? path.join(localConfig.projectRoot, docsLocalDir.substring(2))
        : expandHome(docsLocalDir);
    } else {
      localDocsDir = expandHome(docsLocalDir);
    }
    try {
      const src = expandHome(item.sourcePath);
      await fse.copy(src, localDocsDir, {
        overwrite: true,
        filter: (srcPath: string) => !path.basename(srcPath).startsWith('.'),
      });
      log.debug(`Synced docs → ${localDocsDir}`);
    } catch (e) {
      log.warn(`Failed to sync docs: ${(e as Error).message}`);
    }
  }

  async removeItem(_name: string, _teamConfig: HarnessConfig, _localConfig: LocalConfig): Promise<string[]> {
    log.warn('Removing docs is not supported via remove command. Delete from team repo directly.');
    return [];
  }
}
