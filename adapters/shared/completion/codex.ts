import { lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { dirname, isAbsolute, join } from 'node:path'
import { tmpdir } from 'node:os'
import type { ControlledHost } from './completion.ts'
import type { Deadline } from './deadline.ts'
import { runOwned } from './process.ts'

export interface CodexConfig {
  model: 'gpt-5.5'
  provider: { baseUrl: string; apiKey?: string }
}

const SYSTEM_CONFIGURATION = ['/etc/codex/config.toml', '/etc/codex/requirements.toml', '/etc/codex/managed_config.toml']
const MANAGED_PREFERENCES_PROBE = 'ObjC.import("Foundation"); ObjC.bindFunction("CFPreferencesAppValueIsForced", ["bool", ["id", "id"]]); ($.CFPreferencesAppValueIsForced($("config_toml_base64"), $("com.openai.codex")) || $.CFPreferencesAppValueIsForced($("requirements_toml_base64"), $("com.openai.codex"))) ? "present" : "absent"'

/** Injectable metadata check for tests; production always uses the fixed source paths. */
export function assertCodexSystemConfiguration(paths: readonly string[] = SYSTEM_CONFIGURATION, exists: (path: string) => boolean = metadataPresent): void {
  if (paths.some(exists)) throw new Error('controlled Codex completion does not support ambient system/managed configuration')
}

function metadataPresent(path: string): boolean {
  try { lstatSync(path); return true }
  catch (error) {
    if (['ENOENT', 'ENOTDIR'].includes((error as NodeJS.ErrnoException).code ?? '')) return false
    throw error
  }
}

export function classifyCodexManagedPreferences(code: number, stdout: string): void {
  if (code !== 0 || stdout.trim() !== 'absent') throw new Error('controlled Codex completion requires confirmed absence of macOS managed preferences')
}

function object(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error('Codex configuration/result must be an object')
  return value as Record<string, unknown>
}

function keys(value: Record<string, unknown>, allowed: readonly string[]): void {
  if (Object.keys(value).some((key) => !allowed.includes(key))) throw new Error('Codex configuration contains unsupported settings')
}

export function parseCodexConfig(value: unknown): CodexConfig {
  const config = object(value)
  keys(config, ['model', 'provider'])
  if (config.model !== 'gpt-5.5') throw new Error('controlled Codex completion supports only the pinned gpt-5.5 direct tool mode')
  const supplied = config.provider === undefined ? {} : object(config.provider)
  keys(supplied, ['baseUrl', 'apiKey'])
  const baseUrl = supplied.baseUrl ?? 'https://api.openai.com/v1'
  if (typeof baseUrl !== 'string') throw new Error('Codex provider baseUrl must be a string')
  const url = new URL(baseUrl)
  const loopback = ['127.0.0.1', '[::1]', 'localhost'].includes(url.hostname)
  if (url.username || url.password || url.hash || url.search ||
      !(url.protocol === 'https:' && url.hostname === 'api.openai.com' || url.protocol === 'http:' && loopback)) {
    throw new Error('Codex provider must be the official HTTPS API or an explicit local replay endpoint')
  }
  const apiKey = supplied.apiKey
  if (apiKey !== undefined && (typeof apiKey !== 'string' || !apiKey.trim() || apiKey.includes('\0'))) throw new Error('Codex provider apiKey must be a nonempty string')
  if (!loopback && apiKey === undefined) throw new Error('Codex requires an explicit owner API key; live credentials are never loaded')
  return { model: 'gpt-5.5', provider: { baseUrl, ...(typeof apiKey === 'string' ? { apiKey } : {}) } }
}

export function codexArguments(config: CodexConfig, root: string): string[] {
  const provider = `{name="IronLint owner provider",base_url=${JSON.stringify(config.provider.baseUrl)},wire_api="responses",requires_openai_auth=false${config.provider.apiKey ? ',env_key="IRONLINT_CODEX_API_KEY"' : ''}}`
  const args = ['exec', '--json', '--ephemeral', '--ignore-user-config', '--ignore-rules', '--skip-git-repo-check', '--sandbox', 'workspace-write', '--cd', root,
    '-c', `model=${JSON.stringify(config.model)}`, '-c', 'model_provider="ironlint"', '-c', `model_providers.ironlint=${provider}`,
    '-c', 'approval_policy="never"', '-c', 'web_search="disabled"', '-c', 'agents.enabled=false', '-c', 'orchestrator.mcp.enabled=false',
    '-c', 'sandbox_workspace_write.exclude_tmpdir_env_var=true', '-c', 'sandbox_workspace_write.exclude_slash_tmp=true',
    '-c', `projects.${JSON.stringify(root)}.trust_level="untrusted"`]
  for (const feature of ['hooks', 'plugins', 'recommended_plugins', 'shell_tool', 'multi_agent', 'multi_agent_v2', 'code_mode', 'code_mode_only', 'code_mode_host', 'enable_mcp_apps']) args.push('--disable', feature)
  args.push('-')
  return args
}

/** JSONL is a host-owned transport. Model text never supplies its terminal status. */
export function classifyCodexResult(code: number, stdout: string): string {
  if (code !== 0) throw new Error(`Codex exited ${code}; completion is incomplete`)
  const pending = new Set<string>()
  const seen = new Set<string>()
  let thread = false
  let turn = false
  let complete = false
  let text = ''
  for (const line of stdout.split('\n').filter((line) => line.trim())) {
    const event = object(JSON.parse(line))
    if (complete) throw new Error('Codex emitted events after terminal completion')
    if (event.type === 'thread.started') {
      if (thread || turn || typeof event.thread_id !== 'string' || !event.thread_id) throw new Error('Codex thread identity is missing or repeated')
      thread = true
    } else if (event.type === 'turn.started') {
      if (!thread || turn) throw new Error('Codex turn identity is missing or repeated')
      turn = true
    } else if (event.type === 'turn.completed') {
      if (!turn || pending.size) throw new Error('Codex completion has unsettled work')
      const usage = object(event.usage)
      for (const key of ['input_tokens', 'cached_input_tokens', 'output_tokens', 'reasoning_output_tokens']) {
        if (!Number.isSafeInteger(usage[key]) || Number(usage[key]) < 0) throw new Error('Codex completion usage is malformed')
      }
      if (usage.cache_write_input_tokens !== undefined && (!Number.isSafeInteger(usage.cache_write_input_tokens) || Number(usage.cache_write_input_tokens) < 0)) throw new Error('Codex completion usage is malformed')
      complete = true
    } else if (['item.started', 'item.updated', 'item.completed'].includes(String(event.type))) {
      const item = object(event.item)
      if (item.type === 'error') throw new Error(`Codex reported an error: ${String(item.message ?? 'unknown error')}`)
      if (!turn || typeof item.id !== 'string' || !item.id || !['agent_message', 'reasoning', 'todo_list', 'file_change'].includes(String(item.type))) {
        throw new Error('Codex used an unsupported or uncertain tool route')
      }
      if (event.type === 'item.completed') {
        if (seen.has(item.id)) throw new Error('Codex repeated an item completion')
        if (item.type === 'file_change' && item.status !== 'completed') throw new Error('Codex patch did not complete')
        pending.delete(item.id)
        seen.add(item.id)
        if (item.type === 'agent_message') {
          if (typeof item.text !== 'string') throw new Error('Codex assistant text is malformed')
          text = item.text
        }
      } else {
        if (seen.has(item.id)) throw new Error('Codex updated a settled item')
        pending.add(item.id)
      }
    } else throw new Error(`Codex did not complete: ${String(event.type)}`)
  }
  if (!complete) throw new Error('Codex did not emit terminal completion')
  return text
}

function environment(home: string, config?: CodexConfig): NodeJS.ProcessEnv {
  mkdirSync(join(home, 'codex'), { recursive: true })
  mkdirSync(join(home, 'xdg'), { recursive: true })
  return { PATH: `${dirname(process.execPath)}:/usr/bin:/bin`, HOME: home, XDG_CONFIG_HOME: join(home, 'xdg'), CODEX_HOME: join(home, 'codex'),
    ...(config?.provider.apiKey ? { IRONLINT_CODEX_API_KEY: config.provider.apiKey } : {}) }
}

export class CodexHost implements ControlledHost {
  readonly protectedPaths: readonly string[]
  readonly runtime: string
  readonly configuration: string

  constructor(runtime: string, configuration: string) {
    if (!isAbsolute(runtime) || !isAbsolute(configuration)) throw new Error('Codex runtime and configuration must be absolute owner paths')
    this.runtime = runtime
    this.configuration = configuration
    this.protectedPaths = [runtime, configuration]
  }

  async validate(root: string, deadline: Deadline): Promise<void> {
    this.config()
    const home = mkdtempSync(join(tmpdir(), 'ironlint-codex-version-'))
    try {
      await this.unmanaged(root, home, deadline)
      const result = await runOwned({ executable: this.runtime, args: ['--version'], root, deadline, env: environment(home), input: '', maxMs: 10_000 })
      if (result.code !== 0 || new TextDecoder('utf-8', { fatal: true }).decode(result.stdout).trim() !== 'codex-cli 0.159.1') throw new Error('controlled Codex completion requires codex-cli 0.159.1')
    } finally { rmSync(home, { recursive: true, force: true }) }
  }

  async run(prompt: string, root: string, deadline: Deadline): Promise<string> {
    const config = this.config()
    const home = mkdtempSync(join(tmpdir(), 'ironlint-codex-run-'))
    try {
      await this.unmanaged(root, home, deadline)
      const result = await runOwned({ executable: this.runtime, args: codexArguments(config, root), root, deadline, env: environment(home, config), input: prompt })
      if (result.code !== 0) throw new Error(`Codex exited ${result.code}; completion is incomplete\n${result.stderr.toString('utf8').slice(0, 2_000)}`)
      return classifyCodexResult(result.code, new TextDecoder('utf-8', { fatal: true }).decode(result.stdout))
    } finally { rmSync(home, { recursive: true, force: true }) }
  }

  private config(): CodexConfig {
    const bytes = readFileSync(this.configuration)
    if (bytes.length > 64 * 1024) throw new Error('Codex owner configuration exceeded 64 KiB')
    return parseCodexConfig(JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)))
  }

  private async unmanaged(root: string, home: string, deadline: Deadline): Promise<void> {
    assertCodexSystemConfiguration()
    if (process.platform === 'darwin') {
      // Ask the same CoreFoundation forced-value predicate as the pinned
      // runtime. Do not load, serialize, or print any preference value.
      const result = await runOwned({ executable: '/usr/bin/osascript', args: ['-l', 'JavaScript', '-e', MANAGED_PREFERENCES_PROBE], root, deadline, env: environment(home), input: '', maxMs: 10_000 })
      classifyCodexManagedPreferences(result.code, new TextDecoder('utf-8', { fatal: true }).decode(result.stdout))
    }
  }
}
