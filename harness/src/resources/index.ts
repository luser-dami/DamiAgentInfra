import { ResourceHandler } from './base.js';
import { SkillsHandler } from './skills.js';
import { RulesHandler } from './rules.js';
import { DocsHandler } from './docs.js';
import { AgentsHandler } from './agents.js';
import { HooksHandler } from './hooks.js';
import { McpHandler } from './mcp.js';
import type { ResourceType } from '../types.js';

const handlers: Record<ResourceType, ResourceHandler> = {
  skills: new SkillsHandler(),
  rules: new RulesHandler(),
  docs: new DocsHandler(),
  agents: new AgentsHandler(),
  hooks: new HooksHandler(),
  mcp: new McpHandler(),
};

export function getHandler(type: ResourceType): ResourceHandler {
  return handlers[type];
}

export function getAllHandlers(): ResourceHandler[] {
  return Object.values(handlers);
}

export { SkillsHandler, RulesHandler, DocsHandler, AgentsHandler, HooksHandler, McpHandler };
