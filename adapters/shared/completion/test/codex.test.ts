import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { assertCodexSystemConfiguration, classifyCodexManagedPreferences, classifyCodexResult, codexArguments, CodexHost, parseCodexConfig } from '../codex.ts'
import { Deadline } from '../deadline.ts'

const start = { type: 'thread.started', thread_id: 'fresh' }
const turn = { type: 'turn.started' }
const message = { type: 'item.completed', item: { id: 'm', type: 'agent_message', text: 'Done.' } }
const terminal = { type: 'turn.completed', usage: { input_tokens: 0, cached_input_tokens: 0, output_tokens: 0, reasoning_output_tokens: 0 } }
const wire = (...events: unknown[]) => events.map((event) => JSON.stringify(event)).join('\n') + '\n'

test('Codex success requires one fresh explicit terminal completion and settled items', () => {
  assert.equal(classifyCodexResult(0, wire(start, turn, message, terminal)), 'Done.')
  const patch = { id: 'p', type: 'file_change', changes: [{ path: 'src.ts', kind: 'update' }], status: 'completed' }
  assert.equal(classifyCodexResult(0, wire(start, turn, { type: 'item.started', item: { ...patch, status: 'in_progress' } }, { type: 'item.completed', item: patch }, message, terminal)), 'Done.')
  for (const events of [
    [start, turn, message], [start, turn, terminal, terminal], [start, terminal],
    [start, turn, { type: 'error', message: 'failed' }, terminal],
    [start, turn, { type: 'turn.failed', error: { message: 'failed' } }],
    [start, turn, { type: 'item.completed', item: { id: 'e', type: 'error', message: 'metadata missing' } }, terminal],
    [start, turn, { type: 'item.started', item: { id: 'p', type: 'file_change', status: 'in_progress' } }, terminal],
    [start, turn, { type: 'item.completed', item: { id: 'c', type: 'command_execution', status: 'completed', exit_code: 0 } }, terminal],
    [start, turn, { type: 'item.completed', item: { id: 'a', type: 'collab_tool_call', status: 'completed' } }, terminal],
    [start, turn, { type: 'item.completed', item: { ...patch, status: 'failed' } }, terminal],
    [start, turn, terminal, message], [start, turn, { type: 'new.runtime.event' }, terminal],
    [start, turn, { ...terminal, usage: {} }], [start, turn, { ...terminal, usage: { ...terminal.usage, output_tokens: -1 } }],
  ]) assert.throws(() => classifyCodexResult(0, wire(...events)))
  assert.throws(() => classifyCodexResult(1, wire(start, turn, message, terminal)))
  assert.throws(() => classifyCodexResult(0, '{'))
})

test('Codex owner settings reject arbitrary config, credentials, and unsupported models', () => {
  assert.deepEqual(parseCodexConfig({ model: 'gpt-5.5', provider: { baseUrl: 'http://127.0.0.1:9876/v1' } }), { model: 'gpt-5.5', provider: { baseUrl: 'http://127.0.0.1:9876/v1' } })
  for (const value of [ {}, { model: 'gpt-6.1-sol' }, { model: 'gpt-5.5', args: ['--dangerously-bypass-approvals-and-sandbox'] }, { model: 'gpt-5.5', env: { CODEX_HOME: '/live' } }, { model: 'gpt-5.5', provider: { baseUrl: 'file:///tmp/key' } }, { model: 'gpt-5.5', provider: { baseUrl: 'http://example.com/v1' } }, { model: 'gpt-5.5', provider: { baseUrl: 'https://example.com/v1' } } ]) assert.throws(() => parseCodexConfig(value))
})

test('Codex launch disables shell, agents, hooks, plugins, code mode, and writable owner scratch', () => {
  const args = codexArguments(parseCodexConfig({ model: 'gpt-5.5', provider: { apiKey: 'owner-key' } }), '/tmp/candidate')
  for (const flag of ['--json', '--ephemeral', '--ignore-user-config', '--ignore-rules']) assert.ok(args.includes(flag))
  for (const feature of ['hooks', 'plugins', 'shell_tool', 'multi_agent', 'multi_agent_v2', 'code_mode', 'code_mode_only', 'code_mode_host']) assert.ok(args.some((arg, i) => arg === '--disable' && args[i + 1] === feature))
  assert.equal(args[args.indexOf('--sandbox') + 1], 'workspace-write')
  assert.ok(args.includes('sandbox_workspace_write.exclude_tmpdir_env_var=true'))
  assert.ok(args.includes('sandbox_workspace_write.exclude_slash_tmp=true'))
  assert.ok(args.includes('projects."/tmp/candidate".trust_level="untrusted"'))
  assert.ok(!args.includes('--dangerously-bypass-approvals-and-sandbox'))
})

test('Codex rejects ambient system config before it can start external tools', () => {
  const checked: string[] = []
  assertCodexSystemConfiguration(['/owner/test-system-config', '/owner/test-requirements'], (path) => { checked.push(path); return false })
  assert.deepEqual(checked, ['/owner/test-system-config', '/owner/test-requirements'])
  assert.throws(() => assertCodexSystemConfiguration(['/owner/present-system-config'], () => true), /system\/managed/)
  assert.throws(() => assertCodexSystemConfiguration(['/owner/unreadable-system-config'], () => { throw new Error('access denied') }))
})

test('Codex production metadata predicate treats a dangling system config symlink as present', (t) => {
  const base = mkdtempSync(join(tmpdir(), 'ironlint-system-metadata-'))
  t.after(() => rmSync(base, { recursive: true, force: true }))
  const path = join(base, 'config.toml')
  symlinkSync(join(base, 'missing.toml'), path)
  assert.throws(() => assertCodexSystemConfiguration([path]), /system\/managed/)
  assert.doesNotThrow(() => assertCodexSystemConfiguration([join(base, 'absent.toml')]))
})

test('Codex macOS managed preference preflight requires confirmed absence without reading values', () => {
  assert.doesNotThrow(() => classifyCodexManagedPreferences(0, 'absent\n'))
  for (const [code, output] of [[0, 'present'], [1, 'absent'], [0, ''], [0, 'unknown']] as [number, string][]) assert.throws(() => classifyCodexManagedPreferences(code, output), /managed/)
})

test('Codex rejects malformed owner UTF-8 before executing any runtime command', async (t) => {
  const base = mkdtempSync(join(tmpdir(), 'ironlint-codex-owner-encoding-'))
  t.after(() => rmSync(base, { recursive: true, force: true }))
  const root = join(base, 'work')
  mkdirSync(root)
  const configuration = join(base, 'owner.json')
  const runtime = join(base, 'runtime')
  const marker = join(base, 'runtime-started')
  writeFileSync(configuration, Buffer.concat([Buffer.from('{"model":"gpt-5.5","provider":{"apiKey":"owner'), Buffer.from([0xff]), Buffer.from('key"}}')]))
  writeFileSync(runtime, `#!/bin/sh\ntouch '${marker}'\nprintf 'codex-cli 0.159.1\\n'\n`, { mode: 0o755 })
  const deadline = new Deadline(15_000)
  try {
    await assert.rejects(new CodexHost(runtime, configuration).validate(root, deadline), TypeError)
    assert.equal(existsSync(marker), false)
  } finally { deadline.dispose() }
})
