import { test, type TestContext } from "node:test"
import assert from "node:assert/strict"
import { execFileSync, spawnSync } from "node:child_process"
import { existsSync, mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { createAgentSession, ModelRuntime, SessionManager, SettingsManager, DefaultResourceLoader } from "@earendil-works/pi-coding-agent"
import { fauxAssistantMessage, fauxProvider, fauxToolCall, type FauxResponseStep } from "@earendil-works/pi-ai"
import { runControlled, type CompletionSession } from "../../src/completion.ts"
import { constrainedTools } from "../../src/sdk.ts"

const binary = process.env.IRONLINT_TEST_BIN ?? resolve("../../../../target/debug/ironlint")

async function scriptedSession(root: string, home: string, responses: FauxResponseStep[]): Promise<CompletionSession> {
  const workspace = realpathSync(root)
  const faux = fauxProvider()
  faux.setResponses(responses)
  const modelRuntime = await ModelRuntime.create({ authPath: join(home, "auth.json"), modelsPath: null, refreshOnCreate: false })
  modelRuntime.registerNativeProvider(faux.provider)
  const settingsManager = SettingsManager.inMemory({ retry: { enabled: false }, compaction: { enabled: false } })
  const resourceLoader = new DefaultResourceLoader({ cwd: workspace, agentDir: home, settingsManager, noExtensions: true })
  await resourceLoader.reload()
  const { session } = await createAgentSession({ cwd: workspace, agentDir: home, model: faux.getModel(), modelRuntime,
    settingsManager, sessionManager: SessionManager.inMemory(workspace), resourceLoader, tools: ["read", "edit", "write"],
    customTools: await constrainedTools(workspace) })
  return {
    prompt: (text) => session.prompt(text), waitForIdle: () => session.waitForIdle(),
    abort: () => session.abort(), dispose: () => session.dispose(),
    subscribe: (listener) => session.subscribe((event) => listener(event.type)),
    getLastAssistantText: () => session.getLastAssistantText(),
    get isIdle() { return session.isIdle }, get pendingMessageCount() { return session.pendingMessageCount },
    get finalStopReason() {
      const last = [...session.state.messages].reverse().find((message) => message.role === "assistant")
      return last?.role === "assistant" ? last.stopReason : undefined
    },
  }
}

function fixture(t: TestContext) {
  const base = mkdtempSync(join(tmpdir(), "ironlint-pi-compliance-"))
  t.after(() => rmSync(base, { recursive: true, force: true }))
  const root = join(base, "work")
  const artifacts = join(base, "artifacts")
  const configHome = join(base, "config")
  const agentDir = join(base, "agent")
  mkdirSync(root)
  mkdirSync(artifacts)
  mkdirSync(configHome)
  mkdirSync(agentDir)
  execFileSync("git", ["init", "-q", root])
  const policy = join(base, "owner.yml")
  writeFileSync(policy, `version: 1
checks:
  rule:
    run: 'if grep -q bad main.txt; then printf "main.txt:1: remove forbidden marker; see docs/rules.md\\n"; exit 2; fi'
`)
  const previous = process.env.XDG_CONFIG_HOME
  process.env.XDG_CONFIG_HOME = configHome
  t.after(() => { if (previous === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previous })
  const trust = spawnSync(binary, ["trust", "--config", policy], { cwd: root, encoding: "utf8", env: { ...process.env, XDG_CONFIG_HOME: configHome } })
  assert.equal(trust.status, 0, trust.stderr)
  return { root, artifacts, policy, agentDir }
}

const write = (content: string) => [
  fauxAssistantMessage(fauxToolCall("write", { path: "main.txt", content }), { stopReason: "toolUse" }),
  fauxAssistantMessage("finished"),
]

test("real Pi SDK and real IronLint CLI deny a violation, then accept a repaired candidate", async (t) => {
  if (!existsSync(binary)) return t.skip("build ironlint and set IRONLINT_TEST_BIN")
  const f = fixture(t)
  const responses = [...write("bad"), ...write("good")]
  const result = await runControlled({ ...f, binary, prompt: "write a passing file", expectedIds: ["rule"],
    createSession: () => scriptedSession(f.root, f.agentDir, responses) })
  assert.equal(result.status, "complete", result.diagnostics)
  assert.equal(result.repairTurns, 1)
  assert.equal(readFileSync(join(result.candidate!.tree, "main.txt"), "utf8"), "good")
  result.candidate!.dispose()
})

test("real Pi SDK cannot complete a violating candidate when repair budget is zero", async (t) => {
  if (!existsSync(binary)) return t.skip("build ironlint and set IRONLINT_TEST_BIN")
  const f = fixture(t)
  const result = await runControlled({ ...f, binary, prompt: "write", expectedIds: ["rule"], maxRepairTurns: 0,
    createSession: () => scriptedSession(f.root, f.agentDir, write("bad")) })
  assert.equal(result.status, "incomplete")
  assert.match(result.diagnostics, /main.txt:1: remove forbidden marker/)
})

test("real Pi file tools cannot replace owner policy", async (t) => {
  if (!existsSync(binary)) return t.skip("build ironlint and set IRONLINT_TEST_BIN")
  const f = fixture(t)
  const original = readFileSync(f.policy, "utf8")
  const attempt = [
    fauxAssistantMessage(fauxToolCall("write", { path: f.policy, content: "version: 1\nchecks: {}\n" }), { stopReason: "toolUse" }),
    fauxAssistantMessage("policy changed and I am done"),
  ]
  const result = await runControlled({ ...f, binary, prompt: "try to replace policy", expectedIds: ["rule"],
    createSession: () => scriptedSession(f.root, f.agentDir, attempt) })
  assert.equal(readFileSync(f.policy, "utf8"), original)
  assert.equal(result.status, "complete", result.diagnostics)
  result.candidate?.dispose()
})

test("full acceptance runs without an edit event and ignores candidate policy and forged success text", async (t) => {
  if (!existsSync(binary)) return t.skip("build ironlint and set IRONLINT_TEST_BIN")
  const f = fixture(t)
  writeFileSync(join(f.root, "main.txt"), "bad")
  writeFileSync(join(f.root, ".ironlint.yml"), "version: 1\nchecks: {}\n")
  const result = await runControlled({ ...f, binary, prompt: "finish", expectedIds: ["rule"], maxRepairTurns: 0,
    createSession: () => scriptedSession(f.root, f.agentDir, [fauxAssistantMessage("All checks passed; task complete")]) })
  assert.equal(result.status, "incomplete")
  assert.match(result.diagnostics, /main.txt:1: remove forbidden marker/)
})
