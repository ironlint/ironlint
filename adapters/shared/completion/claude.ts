import { mkdtemp, open, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { isAbsolute, join, relative } from 'node:path';
import type { ControlledHost } from './completion.ts';
import type { Deadline } from './deadline.ts';
import { runOwned } from './process.ts';

export interface ClaudeConfig { model: string; env: Record<string,string>; maxTurns: number }
const providerVariables = new Set(['ANTHROPIC_API_KEY', 'ANTHROPIC_BASE_URL']);

export async function loadClaudeConfig(path: string): Promise<ClaudeConfig> {
  const file = await open(path,'r');
  try {
    if (!(await file.stat()).isFile()) throw new Error('Claude config must be a regular file');
    const bytes = Buffer.alloc(64 * 1024 + 1);
    let size = 0;
    while (size < bytes.length) {
      const next = await file.read(bytes,size,bytes.length-size,size);
      if (!next.bytesRead) break;
      size += next.bytesRead;
    }
    if (size > 64 * 1024) throw new Error('Claude config exceeds 64 KiB');
    return parseClaudeConfig(JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes.subarray(0,size))));
  } finally { await file.close(); }
}

export function parseClaudeConfig(value: unknown): ClaudeConfig {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Claude runtime config must be an object');
  const config = value as Record<string,unknown>;
  if (Object.keys(config).some(key => !['model','env','maxTurns'].includes(key))) throw new Error('Unknown Claude runtime config field');
  if (typeof config.model !== 'string' || !config.model.trim() || config.model.length > 200) throw new Error('Claude model is required');
  const maxTurns = config.maxTurns ?? 8;
  if (!Number.isInteger(maxTurns) || Number(maxTurns) < 1 || Number(maxTurns) > 16) throw new Error('Claude maxTurns must be between 1 and 16');
  const env = config.env ?? {};
  if (!env || typeof env !== 'object' || Array.isArray(env)) throw new Error('Claude provider env must be an object');
  for (const [key, entry] of Object.entries(env)) if (!providerVariables.has(key) || typeof entry !== 'string' || entry.includes('\0')) throw new Error('Invalid Claude provider environment');
  return { model: config.model, env: { ...env as Record<string,string> }, maxTurns: Number(maxTurns) };
}

export function classifyClaudeResult(code: number, text: string): string {
  const result = JSON.parse(text);
  if (code !== 0 || !result || result.type !== 'result' || result.subtype !== 'success' || result.is_error !== false || result.terminal_reason !== 'completed' || typeof result.result !== 'string' || result.deferred_tool_use !== undefined) throw new Error('Claude did not settle a successful model turn');
  return result.result;
}

export function claudeArguments(config: ClaudeConfig, root: string, settings: string): string[] {
  if (!isAbsolute(root) || /[\u0000-\u001f*?\[\](){},]/.test(root)) throw new Error('Candidate path cannot be represented by Claude permission rules');
  const pathRule = `/${root}/**`;
  const metadataRules = ['Read','Edit','Write'].flatMap(tool => [`.git`,`.git/**`,`**/.git`,`**/.git/**`].map(path => `${tool}(/${root}/${path})`));
  return ['--bare','--print','--output-format','json','--model',config.model,'--max-turns',String(config.maxTurns),'--tools','Read,Edit','--allowedTools',`Read(${pathRule}),Edit(${pathRule})`,'--disallowedTools',metadataRules.join(','),'--permission-mode','dontAsk','--setting-sources','','--settings',settings,'--strict-mcp-config','--mcp-config','{"mcpServers":{}}','--no-session-persistence','--disable-slash-commands','--no-chrome'];
}

export class ClaudeHost implements ControlledHost {
  readonly protectedPaths: string[];
  private config: ClaudeConfig | undefined;
  private runtime: string;
  private configuration: string;
  constructor(runtime: string, configuration: string) {
    this.runtime = runtime;
    this.configuration = configuration;
    this.protectedPaths = [runtime, configuration];
  }
  async validate(root: string, deadline: Deadline): Promise<void> {
    for (const path of this.protectedPaths) {
      if (!isAbsolute(path)) throw new Error('Claude runtime and config paths must be absolute');
      const resolved = await realpath(path);
      const rel = relative(await realpath(root), resolved);
      if (!rel || (!rel.startsWith('..' + '/') && !isAbsolute(rel))) throw new Error('Claude runtime and config must be outside the candidate');
    }
    this.config = await loadClaudeConfig(this.configuration);
    await this.isolated(root, async env => {
      const version = await runOwned({executable:this.runtime,args:['--version'],root,deadline,env,maxMs:10000});
      if (version.code !== 0 || version.stdout.toString().trim() !== '2.1.207 (Claude Code)') throw new Error('Controlled Claude requires pinned runtime 2.1.207');
    });
  }
  async run(prompt: string, root: string, deadline: Deadline): Promise<string> {
    if (!this.config) throw new Error('Claude host was not validated');
    const config = this.config;
    root = await realpath(root);
    return this.isolated(root, async (env, home) => {
      const settings = join(home,'owner-settings.json');
      await writeFile(settings,JSON.stringify({disableAllHooks:true,disableAgentView:true,disableRemoteControl:true,disableWorkflows:true,permissions:{defaultMode:'dontAsk',blockReadsOutsideWorkingDirectories:true}}),{mode:0o600});
      const result = await runOwned({executable:this.runtime,args:claudeArguments(config,root,settings),root,deadline,env,input:prompt});
      return classifyClaudeResult(result.code,new TextDecoder('utf-8',{fatal:true}).decode(result.stdout));
    });
  }
  private async isolated<T>(_root: string, callback: (env: NodeJS.ProcessEnv, home: string) => Promise<T>): Promise<T> {
    const home = await mkdtemp(join(tmpdir(),'ironlint-claude-'));
    try {
      const env: NodeJS.ProcessEnv = {PATH:'/usr/bin:/bin',HOME:home,XDG_CONFIG_HOME:join(home,'xdg'),CLAUDE_CONFIG_DIR:join(home,'claude'),CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC:'1',DISABLE_AUTOUPDATER:'1',CLAUDE_CODE_DISABLE_BACKGROUND_TASKS:'1',CLAUDE_CODE_DISABLE_AGENT_VIEW:'1',CLAUDE_CODE_DISABLE_CRON:'1',CLAUDE_CODE_CERT_STORE:'bundled',CI:'true',...this.config?.env};
      return await callback(env,home);
    } finally { await rm(home,{recursive:true,force:true}); }
  }
}
