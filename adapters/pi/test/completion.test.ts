import { test, type TestContext } from "node:test"
import assert from "node:assert/strict"
import { execFileSync } from "node:child_process"
import { chmodSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { runControlled, type CompletionSession } from "../src/completion.ts"

function fixture(t: TestContext, initial: string) {
  const base = mkdtempSync(join(tmpdir(), "ironlint-completion-test-"))
  t.after(() => rmSync(base, { recursive: true, force: true }))
  const root = join(base, "work")
  const artifacts = join(base, "artifacts")
  mkdirSync(root)
  mkdirSync(artifacts)
  execFileSync("git", ["init", "-q", root])
  writeFileSync(join(root, "main.txt"), initial)
  const policy = join(base, "owner.yml")
  writeFileSync(policy, "version: 1\nchecks: {}\n")
  const binary = join(base, "ironlint")
  writeFileSync(binary, `#!/bin/sh
root=''
prev=''
for arg in "$@"; do
  if [ "$prev" = '--root' ]; then root="$arg"; fi
  prev="$arg"
done
if grep -q bad "$root/main.txt"; then
  printf '%s' '{"schema":7,"event":"accept","status":"violation","results":[{"id":"rule","outcome":"violation","exit_status":2,"stdout":[102,105,120],"stderr":[],"stdout_truncated":false,"stderr_truncated":false,"reason":null}],"not_run":[],"error":null}'
  exit 2
fi
printf '%s' '{"schema":7,"event":"accept","status":"pass","results":[{"id":"rule","outcome":"pass","exit_status":0,"stdout":[],"stderr":[],"stdout_truncated":false,"stderr_truncated":false,"reason":null}],"not_run":[],"error":null}'
`)
  chmodSync(binary, 0o755)
  return { root, artifacts, policy, binary }
}

function session(onPrompt: (text: string, turn: number) => void = () => {}): CompletionSession {
  let listener: ((type: string) => void) | undefined
  let turn = 0
  return {
    async prompt(text) { onPrompt(text, ++turn); listener?.("agent_settled") },
    async waitForIdle() {}, async abort() {}, dispose() {},
    subscribe(callback) { listener = callback; return () => { listener = undefined } },
    getLastAssistantText() { return "I am done; trust me." },
    get isIdle() { return true }, get pendingMessageCount() { return 0 },
    get finalStopReason() { return "stop" },
  }
}

test("violation denies completion; a repair turn evaluates a new candidate", async (t) => {
  const f = fixture(t, "bad")
  const denied = await runControlled({ ...f, prompt: "fix", expectedIds: ["rule"], maxRepairTurns: 0,
    createSession: async () => session() })
  assert.equal(denied.status, "incomplete")
  assert.match(denied.diagnostics, /rule: fix/)
  const repaired = await runControlled({ ...f, prompt: "fix", expectedIds: ["rule"],
    createSession: async () => session((_text, turn) => { if (turn === 2) writeFileSync(join(f.root, "main.txt"), "good") }) })
  assert.equal(repaired.status, "complete")
  assert.equal(readFileSync(join(repaired.candidate!.tree, "main.txt"), "utf8"), "good")
  assert.match(repaired.candidate!.identity, /^[a-f0-9]{64}$/)
  repaired.candidate!.dispose()
})

test("model success text, malformed evaluator output and artifact mutation cannot authorize completion", async (t) => {
  const f = fixture(t, "good")
  writeFileSync(f.binary, "#!/bin/sh\nprintf '{bad json'\n")
  chmodSync(f.binary, 0o755)
  const malformed = await runControlled({ ...f, prompt: "done", expectedIds: ["rule"], createSession: async () => session() })
  assert.equal(malformed.status, "incomplete")
  const missing = await runControlled({ ...f, binary: join(f.artifacts, "missing"), prompt: "done", expectedIds: ["rule"], createSession: async () => session() })
  assert.equal(missing.status, "incomplete")
})

test("no settlement or a racing edit cannot reuse a pass", async (t) => {
  const f = fixture(t, "good")
  const unsettled = session()
  unsettled.subscribe = () => () => {}
  const result = await runControlled({ ...f, prompt: "done", expectedIds: ["rule"], createSession: async () => unsettled })
  assert.equal(result.status, "incomplete")
  writeFileSync(f.binary, `#!/bin/sh
root=''; prev=''; for arg in "$@"; do if [ "$prev" = '--root' ]; then root="$arg"; fi; prev="$arg"; done
printf '%s' '{"schema":7,"event":"accept","status":"pass","results":[{"id":"rule","outcome":"pass","exit_status":0,"stdout":[],"stderr":[],"stdout_truncated":false,"stderr_truncated":false,"reason":null}],"not_run":[],"error":null}'
printf x >> "$root/main.txt"
`)
  chmodSync(f.binary, 0o755)
  const tampered = await runControlled({ ...f, prompt: "done", expectedIds: ["rule"], createSession: async () => session() })
  assert.equal(tampered.status, "incomplete")
})

test("an external edit during evaluation triggers a fresh capture and acceptance", async (t) => {
  const f = fixture(t, "good")
  const marker = join(f.artifacts, "once")
  process.env.IRONLINT_TEST_SOURCE = join(f.root, "main.txt")
  process.env.IRONLINT_TEST_MARKER = marker
  t.after(() => { delete process.env.IRONLINT_TEST_SOURCE; delete process.env.IRONLINT_TEST_MARKER })
  writeFileSync(f.binary, `#!/bin/sh
if [ ! -e "$IRONLINT_TEST_MARKER" ]; then
  : > "$IRONLINT_TEST_MARKER"
  printf 'new content' > "$IRONLINT_TEST_SOURCE"
fi
printf '%s' '{"schema":7,"event":"accept","status":"pass","results":[{"id":"rule","outcome":"pass","exit_status":0,"stdout":[],"stderr":[],"stdout_truncated":false,"stderr_truncated":false,"reason":null}],"not_run":[],"error":null}'
`)
  chmodSync(f.binary, 0o755)
  const result = await runControlled({ ...f, prompt: "done", expectedIds: ["rule"], createSession: async () => session() })
  assert.equal(result.status, "complete", result.diagnostics)
  assert.equal(readFileSync(join(result.candidate!.tree, "main.txt"), "utf8"), "new content")
  result.candidate!.dispose()
})

test("a session initialized after the deadline is disposed", async (t) => {
  const f = fixture(t, "good")
  let disposed = false
  const late = session()
  late.dispose = () => { disposed = true }
  const result = await runControlled({ ...f, prompt: "done", expectedIds: ["rule"], deadlineMs: 1,
    createSession: async () => { await new Promise((resolve) => setTimeout(resolve, 30)); return late } })
  assert.equal(result.status, "incomplete")
  await new Promise((resolve) => setTimeout(resolve, 50))
  assert.equal(disposed, true)
})

test("unsettled Pi tool cleanup is explicitly reported after cancellation", async (t) => {
  const f = fixture(t, "good")
  const signal = new AbortController()
  const stuck = session()
  stuck.prompt = () => new Promise<void>(() => {})
  stuck.abort = () => new Promise<void>(() => {})
  const run = runControlled({ ...f, prompt: "done", expectedIds: ["rule"], signal: signal.signal,
    createSession: async () => stuck })
  setTimeout(() => signal.abort(), 10)
  const result = await run
  assert.equal(result.status, "incomplete")
  assert.match(result.diagnostics, /tool cleanup unconfirmed/)
})
