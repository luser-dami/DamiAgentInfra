import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import YAML from 'yaml';
import { loadTagsConfig, filterByTags, collectTagStats, saveTagsConfig } from '../utils/tags.js';
import type { TagsConfig, ResourceItem } from '../types.js';

// ─── Test helpers ──────────────────────────────────────────

function makeTmpDir(): string {
    return fs.mkdtempSync(path.join(os.tmpdir(), 'dami-harness-tags-test-'));
}

function makeItem(name: string, type: 'skills' | 'rules' = 'skills'): ResourceItem {
    return {
        name,
        type,
        sourcePath: `/fake/path/${name}`,
        relativePath: `${type}/${name}`,
    };
}

// ─── loadTagsConfig ────────────────────────────────────────

describe('loadTagsConfig', () => {
    let tmpDir: string;

    beforeEach(() => {
        tmpDir = makeTmpDir();
    });

    afterEach(() => {
        fs.rmSync(tmpDir, { recursive: true, force: true });
    });

    it('returns null when tags.yaml is missing', async () => {
        const result = await loadTagsConfig(tmpDir);
        expect(result).toBeNull();
    });

    it('parses valid tags.yaml', async () => {
        const content = YAML.stringify({
            skills: {
                'hai-deploy': ['hai', 'infra'],
                'nv-benchmark': ['gpu', 'benchmark'],
            },
            rules: {
                'python/tencent_standard': ['python'],
            },
        });
        fs.writeFileSync(path.join(tmpDir, 'tags.yaml'), content);

        const result = await loadTagsConfig(tmpDir);
        expect(result).not.toBeNull();
        expect(result!.skills['hai-deploy']).toEqual(['hai', 'infra']);
        expect(result!.skills['nv-benchmark']).toEqual(['gpu', 'benchmark']);
        expect(result!.rules['python/tencent_standard']).toEqual(['python']);
    });

    it('returns null for malformed YAML', async () => {
        fs.writeFileSync(path.join(tmpDir, 'tags.yaml'), '{{invalid yaml');

        const result = await loadTagsConfig(tmpDir);
        expect(result).toBeNull();
    });

    it('handles empty tags.yaml', async () => {
        fs.writeFileSync(path.join(tmpDir, 'tags.yaml'), '');

        const result = await loadTagsConfig(tmpDir);
        expect(result).toBeNull();
    });

    it('handles tags.yaml with only skills section', async () => {
        const content = YAML.stringify({
            skills: { foo: ['bar'] },
        });
        fs.writeFileSync(path.join(tmpDir, 'tags.yaml'), content);

        const result = await loadTagsConfig(tmpDir);
        expect(result).not.toBeNull();
        expect(result!.skills['foo']).toEqual(['bar']);
        expect(Object.keys(result!.rules)).toHaveLength(0);
    });
});

// ─── filterByTags ──────────────────────────────────────────

describe('filterByTags', () => {
    const items: ResourceItem[] = [
        makeItem('hai-deploy'),
        makeItem('nv-benchmark'),
        makeItem('frontend-design'),
        makeItem('universal-tool'),
    ];

    const tagsConfig: TagsConfig = {
        skills: {
            'hai-deploy': ['hai', 'infra'],
            'nv-benchmark': ['gpu', 'benchmark'],
            'frontend-design': ['frontend', 'ui'],
            // 'universal-tool' is NOT in tagsConfig → untagged
        },
        rules: {},
    };

    it('includes all items when tagsConfig is null', () => {
        const result = filterByTags(items, null, ['hai'], 'skills');
        expect(result.included).toHaveLength(4);
        expect(result.skipped).toHaveLength(0);
    });

    it('includes all items when subscribedTags is undefined', () => {
        const result = filterByTags(items, tagsConfig, undefined, 'skills');
        expect(result.included).toHaveLength(4);
        expect(result.skipped).toHaveLength(0);
    });

    it('includes all items when subscribedTags is empty array', () => {
        const result = filterByTags(items, tagsConfig, [], 'skills');
        expect(result.included).toHaveLength(4);
        expect(result.skipped).toHaveLength(0);
    });

    it('includes matching items and untagged items', () => {
        const result = filterByTags(items, tagsConfig, ['hai'], 'skills');
        const names = result.included.map((i) => i.name);
        expect(names).toContain('hai-deploy');
        expect(names).toContain('universal-tool'); // untagged
        expect(names).not.toContain('nv-benchmark');
        expect(names).not.toContain('frontend-design');
        expect(result.skipped).toHaveLength(2);
    });

    it('includes items with partial tag match', () => {
        const result = filterByTags(items, tagsConfig, ['gpu'], 'skills');
        const names = result.included.map((i) => i.name);
        expect(names).toContain('nv-benchmark');
        expect(names).toContain('universal-tool');
        expect(result.included).toHaveLength(2);
    });

    it('supports multiple subscribed tags (union)', () => {
        const result = filterByTags(items, tagsConfig, ['hai', 'frontend'], 'skills');
        const names = result.included.map((i) => i.name);
        expect(names).toContain('hai-deploy');
        expect(names).toContain('frontend-design');
        expect(names).toContain('universal-tool');
        expect(names).not.toContain('nv-benchmark');
    });

    it('filters rules using rules section of tagsConfig', () => {
        const ruleItems = [
            makeItem('python/tencent_standard', 'rules'),
            makeItem('common/coding-style', 'rules'),
        ];
        const config: TagsConfig = {
            skills: {},
            rules: {
                'python/tencent_standard': ['python'],
                // 'common/coding-style' is untagged
            },
        };

        const result = filterByTags(ruleItems, config, ['golang'], 'rules');
        const names = result.included.map((i) => i.name);
        expect(names).toContain('common/coding-style'); // untagged
        expect(names).not.toContain('python/tencent_standard');
    });
});

// ─── collectTagStats ───────────────────────────────────────

describe('collectTagStats', () => {
    it('collects tag counts across skills and rules', () => {
        const config: TagsConfig = {
            skills: {
                'hai-deploy': ['hai', 'infra'],
                'hai-log': ['hai', 'debug'],
                'nv-bench': ['gpu'],
            },
            rules: {
                'python/std': ['python'],
                'common/style': ['common'],
            },
        };

        const stats = collectTagStats(config);
        expect(stats.get('hai')?.skills).toBe(2);
        expect(stats.get('infra')?.skills).toBe(1);
        expect(stats.get('gpu')?.skills).toBe(1);
        expect(stats.get('python')?.rules).toBe(1);
        expect(stats.get('common')?.rules).toBe(1);
    });
});

// ─── saveTagsConfig ────────────────────────────────────────

describe('saveTagsConfig', () => {
    let tmpDir: string;

    beforeEach(() => {
        tmpDir = makeTmpDir();
    });

    afterEach(() => {
        fs.rmSync(tmpDir, { recursive: true, force: true });
    });

    it('writes tags.yaml and can be read back', async () => {
        const config: TagsConfig = {
            skills: { 'hai-deploy': ['hai', 'infra'] },
            rules: { 'python/std': ['python'] },
        };

        await saveTagsConfig(tmpDir, config);

        const readBack = await loadTagsConfig(tmpDir);
        expect(readBack).not.toBeNull();
        expect(readBack!.skills['hai-deploy']).toEqual(['hai', 'infra']);
        expect(readBack!.rules['python/std']).toEqual(['python']);
    });
});
