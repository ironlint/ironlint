import { test, type TestContext } from "node:test"
import assert from "node:assert/strict"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { createAgentSession, ModelRuntime, SessionManager, SettingsManager, DefaultResourceLoader } from "@earendil-works/pi-coding-agent"
import { fauxAssistantMessage, fauxProvider, type FauxResponseStep } from "@earendil-works/pi-ai"

async function runtime(t: TestContext, responses: FauxResponseStep[]) {
  const root = mkdtempSync(join(tmpdir(), "ironlint-pi-runtime-"))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const faux = fauxProvider()
  faux.setResponses(responses)
  const modelRuntime = await ModelRuntime.create({ authPath: join(root, "auth.json"), modelsPath: null, refreshOnCreate: false })
  modelRuntime.registerNativeProvider(faux.provider)
  const settingsManager = SettingsManager.inMemory({ retry: { enabled: false }, compaction: { enabled: false } })
  const resourceLoader = new DefaultResourceLoader({ cwd: root, agentDir: join(root, "agent"), settingsManager, noExtensions: true })
  await resourceLoader.reload()
  const { session } = await createAgentSession({
    cwd: root, agentDir: join(root, "agent"), model: faux.getModel(), modelRuntime,
    settingsManager, sessionManager: SessionManager.inMemory(root), resourceLoader, noTools: "all",
  })
  t.after(() => session.dispose())
  return { session, faux }
}

test("Pi 0.87.1 emits agent_settled after the final assistant response", async (t) => {
  const { session } = await runtime(t, [fauxAssistantMessage("clean")])
  const events: string[] = []
  const unsubscribe = session.subscribe((event) => events.push(event.type))
  await session.prompt("finish")
  await session.waitForIdle()
  unsubscribe()
  assert.equal(session.getLastAssistantText(), "clean")
  assert.equal(session.isIdle, true)
  assert.ok(events.indexOf("agent_end") >= 0)
  assert.ok(events.indexOf("agent_settled") > events.indexOf("agent_end"))
})

test("queued follow-up runs before final settlement", async (t) => {
  let release!: () => void
  const hold = new Promise<void>((resolve) => { release = resolve })
  const { session } = await runtime(t, [async () => { await hold; return fauxAssistantMessage("first") }, fauxAssistantMessage("second")])
  const events: string[] = []
  const unsubscribe = session.subscribe((event) => events.push(event.type))
  const running = session.prompt("first")
  for (let i = 0; i < 100 && !session.isStreaming; i++) await new Promise((resolve) => setTimeout(resolve, 5))
  assert.equal(session.isStreaming, true)
  session.followUp("second")
  release()
  await running
  await session.waitForIdle()
  unsubscribe()
  assert.equal(session.getLastAssistantText(), "second")
  assert.equal(events.at(-1), "agent_settled")
})

test("abort settles without yielding a clean task result", async (t) => {
  let release!: () => void
  const hold = new Promise<void>((resolve) => { release = resolve })
  const { session } = await runtime(t, [async () => { await hold; return fauxAssistantMessage("late") }])
  const running = session.prompt("wait")
  for (let i = 0; i < 100 && !session.isStreaming; i++) await new Promise((resolve) => setTimeout(resolve, 5))
  const aborting = session.abort()
  release()
  await Promise.allSettled([running, aborting])
  assert.equal(session.isIdle, true)
  const last = session.state.messages.at(-1)
  assert.notEqual(last?.role === "assistant" ? last.stopReason : undefined, "stop")
})
