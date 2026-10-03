import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { tmpdir } from 'node:os'
import { createServer } from 'node:http'
import { execFileSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import test from 'node:test'
import { CodexHost } from '../codex.ts'
import { Deadline } from '../deadline.ts'
import { runOwned } from '../process.ts'
import { runControlled } from '../completion.ts'

const runtime = process.env.IRONLINT_CODEX_RUNTIME
const binary = process.env.IRONLINT_TEST_BIN ?? process.env.IRONLINT_BIN
const enabled = Boolean(runtime)
type Response = 'final' | 'patch' | 'error' | 'stall' | 'shell' | 'owner'

async function fixture(t: { after: (fn: () => void | Promise<void>) => void }, responses: Response[]) {
  const base = mkdtempSync(join(tmpdir(), 'ironlint-codex-contract-'))
  t.after(() => rmSync(base, { recursive: true, force: true }))
  const root = join(base, 'work')
  const home = join(base, 'home')
  const artifacts = join(base, 'artifacts')
  for (const dir of [root, home, artifacts]) mkdirSync(dir)
  execFileSync('git', ['init', '-q'], { cwd: root })
  const requests: Record<string, unknown>[] = []
  const sockets = new Set<import('node:net').Socket>()
  const configuration = join(base, 'owner.json')
  const server = createServer((request, reply) => {
    if (request.method !== 'POST') { reply.writeHead(200, { 'content-type': 'application/json' }); reply.end('{"data":[]}'); return }
    const chunks: Buffer[] = []
    request.on('data', (chunk: Buffer) => chunks.push(chunk))
    request.on('end', () => {
      requests.push(JSON.parse(Buffer.concat(chunks).toString('utf8')))
      const mode = responses[Math.min(requests.length - 1, responses.length - 1)]
      if (mode === 'stall') return
      if (mode === 'error') { reply.writeHead(400, { 'content-type': 'application/json' }); reply.end('{"error":{"message":"fixture unavailable","type":"invalid_request_error","code":"invalid_request"}}'); return }
      const rid = `resp_${requests.length}`
      const item = mode === 'final' ? { type: 'message', role: 'assistant', id: rid, content: [{ type: 'output_text', text: 'done' }] }
        : mode === 'shell' ? { type: 'function_call', name: 'exec_command', call_id: rid, arguments: JSON.stringify({ cmd: 'touch background.txt' }) }
        : { type: 'custom_tool_call', name: 'apply_patch', call_id: rid, input: mode === 'owner' ? `*** Begin Patch\n*** Update File: ${configuration}\n@@\n-${readFileSync(configuration, 'utf8')}\n+weakened\n*** End Patch` : '*** Begin Patch\n*** Add File: pass.txt\n+yes\n*** End Patch' }
      const events = [{ type: 'response.created', response: { id: rid } }, { type: 'response.output_item.done', item }, { type: 'response.completed', response: { id: rid, usage: { input_tokens: 0, output_tokens: 0, total_tokens: 0 } } }]
      reply.writeHead(200, { 'content-type': 'text/event-stream' })
      reply.end(events.map((event) => `event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`).join(''))
    })
  })
  server.on('connection', (socket) => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)) })
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  t.after(async () => { for (const socket of sockets) socket.destroy(); await new Promise<void>((resolve) => server.close(() => resolve())) })
  const address = server.address() as import('node:net').AddressInfo
  const baseUrl = `http://127.0.0.1:${address.port}/v1`
  writeFileSync(configuration, JSON.stringify({ model: 'gpt-5.5', provider: { baseUrl } }))
  const host = new CodexHost(runtime!, configuration)
  return { base, root, home, artifacts, requests, configuration, host, baseUrl }
}

test('pinned Codex runtime settles clean text and synchronous patch work with restricted tools', { skip: !enabled }, async (t) => {
  for (const responses of [['final'], ['patch', 'final']] as Response[][]) {
    const f = await fixture(t, responses)
    const deadline = new Deadline(30_000)
    try {
      await f.host.validate(f.root, deadline)
      const result = await f.host.run('finish the task', f.root, deadline).catch((error) => {
        // Preserve only fixture tool-output diagnostics, never configuration,
        // provider credentials, or the model request's instruction content.
        const input = f.requests.at(-1)?.input
        if (Array.isArray(input)) t.diagnostic(JSON.stringify(input.filter((item) => item?.type === 'custom_tool_call_output')))
        throw error
      })
      assert.equal(result, 'done')
      assert.equal(existsSync(join(f.root, 'pass.txt')), responses[0] === 'patch')
      const names = (f.requests[0].tools as { name?: string }[]).map((tool) => tool.name).filter(Boolean)
      assert.ok(names.includes('apply_patch'))
      for (const forbidden of ['exec', 'exec_command', 'shell_command', 'spawn_agent', 'mcp__filesystem__write_file']) assert.ok(!names.includes(forbidden))
    } finally { deadline.dispose() }
  }
})

test('pinned Codex runtime errors/cancellation withhold completion and excluded tools cannot write', { skip: !enabled }, async (t) => {
  for (const responses of [['error'], ['stall'], ['shell', 'final'], ['owner', 'final']] as Response[][]) {
    const f = await fixture(t, responses)
    const original = readFileSync(f.configuration)
    const deadline = new Deadline(responses[0] === 'stall' ? 3_000 : 30_000)
    try {
      if (['shell', 'owner'].includes(responses[0])) assert.equal(await f.host.run('finish the task', f.root, deadline), 'done')
      else await assert.rejects(f.host.run('finish the task', f.root, deadline), undefined, responses[0])
      assert.ok(!existsSync(join(f.root, 'background.txt')))
      assert.deepEqual(readFileSync(f.configuration), original)
    } finally { deadline.dispose() }
  }
})

test('pinned Codex completion controller owns clean, repair, exhausted, infrastructure, and candidate mutation outcomes', { skip: !enabled || !binary }, async (t) => {
  for (const [responses, clean, status, repairs, mutation] of [
    [['final'], true, 'complete', 0], [['final', 'patch', 'final'], false, 'complete', 1],
    [['final'], false, 'incomplete', 1], [['error'], true, 'incomplete', 0],
    [['final'], true, 'incomplete', 0, 'artifact'], [['final'], true, 'incomplete', 0, 'source'],
  ] as [Response[], boolean, string, number, string?][]) {
    const f = await fixture(t, responses)
    if (clean) writeFileSync(join(f.root, 'pass.txt'), 'yes')
    const policy = join(f.base, 'policy.yml')
    const command = mutation === 'artifact' ? 'echo changed > pass.txt' : mutation === 'source' ? `printf x >> ${JSON.stringify(join(f.root, 'late.txt'))}` : 'test -f pass.txt'
    writeFileSync(policy, `version: 1\nchecks:\n  task:\n    on: [accept]\n    run: ${JSON.stringify(command)}\n`)
    // Consent belongs solely to this probe's isolated store.
    execFileSync(binary!, ['trust', '--config', policy], { cwd: f.root, env: { ...process.env, HOME: f.home, XDG_CONFIG_HOME: join(f.home, 'xdg') } })
    const oldXdg = process.env.XDG_CONFIG_HOME
    process.env.XDG_CONFIG_HOME = join(f.home, 'xdg')
    try {
      const result = await runControlled({ root: f.root, policy, binary: binary!, artifacts: f.artifacts, expectedIds: ['task'], prompt: 'finish', host: f.host, maxRepairTurns: 1, deadlineMs: 30_000 })
      assert.equal(result.status, status, result.diagnostics)
      assert.equal(result.repairTurns, repairs)
      if (result.candidate) { assert.ok(existsSync(join(result.candidate.tree, 'pass.txt'))); result.candidate.dispose() }
    } finally { if (oldXdg === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = oldXdg }
  }
})

test('pinned Codex controller cancellation settles without producing a candidate or inherited approval', { skip: !enabled || !binary }, async (t) => {
  const f = await fixture(t, ['stall'])
  const policy = join(f.base, 'policy.yml')
  writeFileSync(policy, 'version: 1\nchecks:\n  task:\n    on: [accept]\n    run: exit 0\n')
  const controller = new AbortController()
  const timer = setInterval(() => { if (f.requests.length) controller.abort(new Error('user cancelled')) }, 10)
  try {
    const result = await runControlled({ root: f.root, policy, binary: binary!, artifacts: f.artifacts, expectedIds: ['task'], prompt: 'finish', host: f.host, signal: controller.signal, deadlineMs: 30_000 })
    assert.equal(result.status, 'incomplete')
    assert.equal(result.candidate, undefined)
    assert.match(result.diagnostics, /cancel/i)
  } finally { clearInterval(timer) }
})

test('pinned Codex explicit launcher owns one successful or exhausted terminal JSON result', { skip: !enabled || !binary }, async (t) => {
  const launcher = fileURLToPath(new URL('../../../codex/bin/ironlint-codex-complete', import.meta.url))
  for (const clean of [true, false]) {
    const f = await fixture(t, ['final'])
    if (clean) writeFileSync(join(f.root, 'pass.txt'), 'yes')
    const policy = join(f.base, 'policy.yml')
    writeFileSync(policy, 'version: 1\nchecks:\n  task:\n    on: [accept]\n    run: test -f pass.txt\n')
    const env = { PATH: `${dirname(process.execPath)}:/usr/bin:/bin`, HOME: f.home, XDG_CONFIG_HOME: join(f.home, 'xdg'), ...(process.env.TMPDIR ? { TMPDIR: process.env.TMPDIR } : {}) }
    execFileSync(binary!, ['trust', '--config', policy], { cwd: f.root, env })
    const deadline = new Deadline(45_000)
    try {
      const result = await runOwned({ executable: '/bin/sh', args: [launcher, '--root', f.root, '--policy', policy, '--binary', binary!, '--artifacts', f.artifacts,
        '--runtime', runtime!, '--runtime-config', f.configuration, '--checks', 'task', '--task', 'finish', '--max-repair-turns', '0', '--deadline-seconds', '30'],
        root: f.root, deadline, env, input: '' })
      assert.equal(result.code, clean ? 0 : 3, result.stderr.toString())
      const lines = new TextDecoder('utf-8', { fatal: true }).decode(result.stdout).trim().split('\n')
      assert.equal(lines.length, 1)
      const terminal = JSON.parse(lines[0])
      assert.equal(terminal.status, clean ? 'complete' : 'incomplete', terminal.diagnostics)
      assert.equal(terminal.repairTurns, 0)
      assert.equal(typeof terminal.diagnostics, 'string')
      if (clean) {
        assert.deepEqual(Object.keys(terminal).sort(), ['candidate', 'diagnostics', 'modelText', 'repairTurns', 'status'])
        assert.deepEqual(Object.keys(terminal.candidate).sort(), ['artifact', 'identity', 'tree'])
        assert.match(terminal.candidate.identity, /^[a-f0-9]{64}$/)
        assert.equal(readFileSync(join(terminal.candidate.tree, 'pass.txt'), 'utf8'), 'yes')
        assert.equal(terminal.modelText, 'done')
        // The standalone result transfers this artifact to its caller.
        rmSync(terminal.candidate.artifact, { recursive: true, force: true })
      } else assert.deepEqual(Object.keys(terminal).sort(), ['diagnostics', 'repairTurns', 'status'])
    } finally { deadline.dispose() }
  }
})

test('pinned Codex controlled route ignores candidate-owned config and native hook registrations', { skip: !enabled }, async (t) => {
  const f = await fixture(t, ['final'])
  mkdirSync(join(f.root, '.codex'))
  const marker = join(f.root, 'unexpected-hook')
  writeFileSync(join(f.root, '.codex', 'hooks.json'), JSON.stringify({ hooks: { Stop: [{ hooks: [{ type: 'command', command: `touch ${JSON.stringify(marker)}` }] }] } }))
  writeFileSync(join(f.root, '.codex', 'config.toml'), '[features]\nhooks=true\nplugins=true\nshell_tool=true\nmulti_agent=true\n')
  const deadline = new Deadline(30_000)
  try {
    assert.equal(await f.host.run('finish', f.root, deadline), 'done')
    assert.equal(existsSync(marker), false)
    const names = (f.requests[0].tools as { name?: string }[]).map((tool) => tool.name)
    assert.ok(!names.includes('exec_command'))
    assert.ok(!names.includes('spawn_agent'))
  } finally { deadline.dispose() }
})

test('pinned native Stop continuation, errors, missing hooks, and competing hooks do not record incomplete', { skip: !enabled }, async (t) => {
  for (const mode of ['continued-warning', 'stderr-block', 'repeated', 'terminal-stop', 'error', 'timeout', 'missing', 'disabled', 'competing']) {
    const f = await fixture(t, ['final'])
    const log = join(f.home, 'payloads.jsonl')
    const script = join(f.home, 'hook.py')
    const source = `import json,sys,time\np=json.load(sys.stdin)\nwith open(${JSON.stringify(log)},'a') as f:f.write(json.dumps(p)+'\\n')\nmode=${JSON.stringify(mode)}\ncount=len(open(${JSON.stringify(log)}).readlines())\nif mode=='error':sys.exit(1)\nif mode=='timeout':time.sleep(10)\nif mode=='stderr-block' and not p['stop_hook_active']:print('Repair required',file=sys.stderr);sys.exit(2)\nif (mode=='terminal-stop' and p['stop_hook_active']) or (mode=='repeated' and count>=4):print(json.dumps({'continue':False,'stopReason':'exhausted'}))\nelif mode in ('continued-warning','stderr-block') and p['stop_hook_active']:print(json.dumps({'systemMessage':'Acceptance incomplete'}))\nelse:print(json.dumps({'decision':'block','reason':'Repair required'}))\n`
    writeFileSync(script, source)
    const hook = { type: 'command', command: `/usr/bin/python3 ${JSON.stringify(script)}`, timeout: 1 }
    if (mode !== 'missing') writeFileSync(join(f.home, 'hooks.json'), JSON.stringify({ hooks: { Stop: [{ hooks: mode === 'competing' ? [hook, { type: 'command', command: 'echo \'{"continue":false,"stopReason":"competing"}\'' }] : [hook] }] } }))
    const deadline = new Deadline(30_000)
    try {
      const args = ['exec', '--json', '--ephemeral', '--skip-git-repo-check', '--dangerously-bypass-hook-trust', '--sandbox', 'read-only', '--cd', f.root,
        '-c', 'model="gpt-5.5"', '-c', 'model_provider="fixture"', '-c', `model_providers.fixture={name="fixture",base_url=${JSON.stringify(f.baseUrl)},wire_api="responses",requires_openai_auth=false}`,
        '--disable', 'plugins', '--disable', 'shell_tool', '--disable', 'multi_agent', '--disable', 'multi_agent_v2', '-']
      if (mode === 'disabled') args.splice(args.length - 1, 0, '--disable', 'hooks')
      const result = await runOwned({ executable: runtime!, args, root: f.root, deadline, env: { PATH: `${dirname(process.execPath)}:/usr/bin:/bin`, HOME: f.home, XDG_CONFIG_HOME: join(f.home, 'xdg'), CODEX_HOME: f.home }, input: 'finish' })
      assert.equal(result.code, 0, result.stderr.toString())
      assert.ok(result.stdout.toString().includes('"type":"turn.completed"'), result.stdout.toString())
      const payloads = existsSync(log) ? readFileSync(log, 'utf8').trim().split('\n').map((line) => JSON.parse(line)) : []
      if (['continued-warning', 'stderr-block', 'terminal-stop'].includes(mode)) assert.deepEqual(payloads.map((p) => p.stop_hook_active), [false, true])
      if (mode === 'repeated') assert.deepEqual(payloads.map((p) => p.stop_hook_active), [false, true, true, true])
      if (mode === 'missing' || mode === 'disabled') assert.equal(payloads.length, 0)
      else assert.ok(payloads.length > 0)
    } finally { deadline.dispose() }
  }
})
