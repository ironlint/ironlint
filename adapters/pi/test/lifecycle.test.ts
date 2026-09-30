import { test, type TestContext } from "node:test"
import assert from "node:assert/strict"
import childProcess, { type ChildProcess } from "node:child_process"
import { EventEmitter } from "node:events"
import { syncBuiltinESMExports } from "node:module"
import { PassThrough } from "node:stream"
import { mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import ironlintExtension, { runIronLint } from "../src/index.ts"

const WALL_MS = 600_000
const GRACE_MS = 2_000
const STDOUT_BYTES = 8 * 1024 * 1024
const STDERR_BYTES = 64 * 1024

class FakeChild extends EventEmitter {
  stdin = new PassThrough()
  stdout = new PassThrough()
  stderr = new PassThrough()
  pid = 123_456_789
  signals: string[] = []
  killError: Error | undefined
  onKill: (() => void) | undefined
  unreferenced = false

  kill(signal: string): boolean {
    this.signals.push(signal)
    if (this.killError) this.emit("error", this.killError)
    this.onKill?.()
    if (this.killError) return false
    return true
  }

  close(code = 0): void {
    this.emit("close", code, null)
  }

  unref(): void { this.unreferenced = true }
}

function children(t: TestContext): FakeChild[] {
  const spawned: FakeChild[] = []
  t.mock.method(childProcess, "spawn", () => {
    const child = new FakeChild()
    child.pid += spawned.length
    spawned.push(child)
    return child as unknown as ChildProcess
  })
  t.mock.method(process, "kill", () => true)
  syncBuiltinESMExports()
  t.after(() => {
    for (const child of spawned) child.close(3)
    t.mock.restoreAll()
    syncBuiltinESMExports()
  })
  return spawned
}

function project(t: TestContext): string {
  const root = mkdtempSync(join(tmpdir(), "ironlint-pi-lifecycle-"))
  writeFileSync(join(root, ".ironlint.yml"), "version: 1\nchecks: {}\n")
  t.after(() => rmSync(root, { recursive: true, force: true }))
  return root
}

function extension(t: TestContext, root = project(t)): (event: unknown) => Promise<unknown> {
  let callback: ((event: unknown) => Promise<unknown>) | undefined
  ironlintExtension({ cwd: root, on(_, handler) { callback = handler as typeof callback } })
  assert.ok(callback)
  return callback
}

const passResult = {
  id: "unicode-🦀", outcome: "pass", exit_status: 0, stdout: [], stderr: [],
  stdout_truncated: false, stderr_truncated: false, reason: null,
}
const pass = { schema: 7, event: "change", status: "pass", results: [passResult], not_run: [], error: null }
const empty = { ...pass, status: "not_run", results: [] }

async function turn(): Promise<void> { await Promise.resolve(); await Promise.resolve() }

test("D7 stdout overflow stops the owned child and discards incomplete JSON", async (t) => {
  const spawned = children(t)
  const pending = runIronLint([], "/tmp")
  spawned[0]!.stdout.write(Buffer.alloc(STDOUT_BYTES + 1, 0x20))
  assert.equal(spawned[0]!.stdin.writableEnded, true)
  assert.deepEqual(spawned[0]!.signals, [])
  spawned[0]!.close()
  const result = await pending
  assert.notEqual(result.exitCode, 0)
  assert.equal(result.stdout, "")
  assert.match(result.stderr, /stdout.*limit/i)
})

test("D7 stderr is byte bounded and marks truncation", async (t) => {
  const spawned = children(t)
  const pending = runIronLint([], "/tmp")
  spawned[0]!.stderr.write(Buffer.alloc(STDERR_BYTES + 100, 0x78))
  spawned[0]!.close(3)
  const result = await pending
  assert.equal(result.stderr.indexOf("\n"), STDERR_BYTES)
  assert.match(result.stderr, /stderr.*truncated/i)
})

test("D7 wall expiry without close settles after grace and survives late events", async (t) => {
  const spawned = children(t)
  t.mock.timers.enable({ apis: ["setTimeout"] })
  let result: Awaited<ReturnType<typeof runIronLint>> | undefined
  void runIronLint([], "/tmp").then((value) => { result = value })
  t.mock.timers.tick(WALL_MS)
  assert.equal(spawned[0]!.stdin.writableEnded, true)
  assert.deepEqual(spawned[0]!.signals, [])
  t.mock.timers.tick(GRACE_MS)
  assert.deepEqual(spawned[0]!.signals, ["SIGKILL"])
  await Promise.resolve()
  assert.ok(result)
  assert.notEqual(result.exitCode, 0)
  assert.match(result.stderr, /closure.*not observed/i)
  assert.equal(spawned[0]!.stdout.destroyed, true)
  assert.equal(spawned[0]!.stderr.destroyed, true)
  assert.doesNotThrow(() => spawned[0]!.emit("error", new Error("late child error")))
  assert.doesNotThrow(() => spawned[0]!.stdout.emit("error", new Error("late pipe error")))
  spawned[0]!.close()
  t.mock.timers.tick(WALL_MS + GRACE_MS)
})

test("D7 newer successful mutation cancels old output without stale feedback", async (t) => {
  const spawned = children(t)
  const callback = extension(t)
  const first = callback({ toolName: "write", input: { path: "old.rs" }, content: [{ type: "text", text: "old edit" }] })
  spawned[0]!.stdout.write("partial old output")
  const second = callback({ toolName: "write", input: { path: "new.rs" }, content: [{ type: "text", text: "new edit" }] })
  assert.equal(spawned[0]!.stdin.writableEnded, true)
  assert.deepEqual(spawned[0]!.signals, [])
  spawned[0]!.close(2)
  spawned[1]!.close(3)
  assert.equal(await first, undefined)
  const feedback = await second as { content: Array<{ text: string }> }
  assert.equal(feedback.content[0]!.text, "new edit")
})

test("D7 malformed successful output is an evaluation diagnostic", async (t) => {
  const spawned = children(t)
  const callback = extension(t)
  const pending = callback({ toolName: "write", input: { path: "a.rs" }, content: [{ type: "text", text: "completed" }] })
  spawned[0]!.stdout.write('{"schema":7')
  spawned[0]!.close()
  const result = await pending as { content: Array<{ text: string }> }
  assert.ok(result)
  assert.equal(result.content[0]!.text, "completed")
  assert.match(result.content.at(-1)!.text, /could not evaluate.*incomplete/i)
})

test("D7 new mutation cancels an old run already waiting for timeout closure", async (t) => {
  const spawned = children(t)
  const callback = extension(t)
  t.mock.timers.enable({ apis: ["setTimeout"] })
  const first = callback({ toolName: "write", input: { path: "old.rs" }, content: [] })
  t.mock.timers.tick(WALL_MS)
  const second = callback({ toolName: "write", input: { path: "new.rs" }, content: [] })
  spawned[0]!.close(3)
  spawned[1]!.close(3)
  assert.equal(await first, undefined)
  await second
})

test("D7 split UTF-8 bytes stay intact in complete bounded stdout and stderr", async (t) => {
  const spawned = children(t)
  const pending = runIronLint([], "/tmp")
  const json = JSON.stringify(pass)
  for (const byte of Buffer.from(json)) spawned[0]!.stdout.write(Buffer.from([byte]))
  for (const byte of Buffer.from("diagnostic 🦀")) spawned[0]!.stderr.write(Buffer.from([byte]))
  spawned[0]!.close()
  const result = await pending
  assert.equal(result.exitCode, 0)
  assert.equal(result.stdout, json)
  assert.equal(result.stderr, "diagnostic 🦀")
})

test("D7 otherwise valid JSON byte-array expansion counts against stdout bytes", async (t) => {
  const spawned = children(t)
  const pending = runIronLint([], "/tmp")
  const prefix = JSON.stringify(pass).replace('"stdout":[]', '"stdout":[255]')
  const json = prefix.replace("[255]", `[${"255,".repeat(STDOUT_BYTES / 4)}255]`)
  assert.ok(Buffer.byteLength(json) > STDOUT_BYTES)
  assert.equal(JSON.parse(json).schema, 7)
  const bytes = Buffer.from(json)
  for (let offset = 0; offset < bytes.length; offset += 64 * 1024) {
    spawned[0]!.stdout.write(bytes.subarray(offset, offset + 64 * 1024))
  }
  spawned[0]!.close()
  const result = await pending
  assert.equal(result.exitCode, 3)
  assert.equal(result.stdout, "")
  assert.equal(spawned[0]!.stdin.writableEnded, true)
  assert.deepEqual(spawned[0]!.signals, [])
})

test("D7 sustained stderr drains excess bytes without stopping a valid run", async (t) => {
  const spawned = children(t)
  const pending = runIronLint([], "/tmp")
  for (let chunk = 0; chunk < 256; chunk++) spawned[0]!.stderr.write(Buffer.alloc(4096, 0x78))
  spawned[0]!.stdout.write(JSON.stringify(empty))
  spawned[0]!.close()
  const result = await pending
  assert.equal(result.exitCode, 0)
  assert.equal(Buffer.byteLength(result.stderr.split("\n")[0]!), STDERR_BYTES)
  assert.match(result.stderr, /truncated/)
  assert.deepEqual(spawned[0]!.signals, [])
})

test("D7 exit0 requires complete consistent schema7 change feedback", async (t) => {
  const spawned = children(t)
  const invalid = [
    {}, { ...pass, schema: 6 }, { ...pass, event: "accept" }, { ...pass, error: "failed" },
    { ...pass, not_run: [{ id: "later", reason: "execution_error" }] },
    { ...pass, status: "not_run" }, { ...pass, results: [] },
    { ...pass, results: [{ ...passResult, outcome: "violation" }] },
    { ...pass, results: [{ ...passResult, stdout: [256] }] },
    { ...pass, results: [{ ...passResult, stderr_truncated: undefined }] },
  ]
  for (const value of invalid) {
    const pending = runIronLint([], "/tmp")
    spawned.at(-1)!.stdout.write(JSON.stringify(value))
    spawned.at(-1)!.close()
    const result = await pending
    assert.equal(result.exitCode, 3)
    assert.match(result.stderr, /incomplete.*schema 7/)
  }
})

test("D7 child error and close race settles once and releases callbacks and timers", async (t) => {
  const spawned = children(t)
  t.mock.timers.enable({ apis: ["setTimeout"] })
  let count = 0
  const pending = runIronLint([], "/tmp").then((result) => { count++; return result })
  spawned[0]!.emit("error", new Error("spawn failed"))
  spawned[0]!.close()
  const result = await pending
  assert.equal(result.exitCode, 3)
  assert.match(result.stderr, /spawn failed/)
  assert.equal(spawned[0]!.listenerCount("close"), 0)
  assert.equal(spawned[0]!.listenerCount("error"), 1)
  assert.equal(spawned[0]!.stdout.listenerCount("data"), 0)
  assert.equal(spawned[0]!.stderr.listenerCount("data"), 0)
  spawned[0]!.emit("error", new Error("late error"))
  spawned[0]!.stderr.emit("error", new Error("late stderr error"))
  spawned[0]!.close(2)
  t.mock.timers.tick(WALL_MS + GRACE_MS)
  await turn()
  assert.equal(count, 1)
  assert.deepEqual(spawned[0]!.signals, [])
})

test("D7 pipe error without close uses the same bounded grace", async (t) => {
  const spawned = children(t)
  t.mock.timers.enable({ apis: ["setTimeout"] })
  const pending = runIronLint([], "/tmp")
  spawned[0]!.stdout.emit("error", new Error("pipe broke"))
  t.mock.timers.tick(GRACE_MS)
  const result = await pending
  assert.match(result.stderr, /pipe broke/)
  assert.match(result.stderr, /closure.*not observed/)
})

test("D7 overflow without close settles after grace", async (t) => {
  const spawned = children(t)
  t.mock.timers.enable({ apis: ["setTimeout"] })
  const pending = runIronLint([], "/tmp")
  spawned[0]!.stdout.write(Buffer.alloc(STDOUT_BYTES + 1))
  t.mock.timers.tick(GRACE_MS)
  const result = await pending
  assert.equal(result.stdout, "")
  assert.match(result.stderr, /limit/)
  assert.match(result.stderr, /closure.*not observed/)
})

test("D7 late old events cannot stop or settle a replacement run", async (t) => {
  const spawned = children(t)
  const callback = extension(t)
  const first = callback({ toolName: "write", input: { path: "old.rs" }, content: [] })
  const second = callback({ toolName: "write", input: { path: "new.rs" }, content: [] })
  spawned[0]!.stdout.write("late cancelled bytes")
  spawned[0]!.close(2)
  assert.equal(await first, undefined)
  spawned[0]!.emit("error", new Error("old late failure"))
  spawned[0]!.close(3)
  assert.deepEqual(spawned[1]!.signals, [])
  spawned[1]!.stdout.write(JSON.stringify(pass))
  spawned[1]!.close()
  assert.equal(await second, undefined)
})

test("D7 completed result racing a new edit retains its superseded indication", async (t) => {
  const spawned = children(t)
  const callback = extension(t)
  const first = callback({ toolName: "write", input: { path: "old.rs" }, content: [{ type: "text", text: "old edit" }] })
  spawned[0]!.stderr.write("old completed failure")
  spawned[0]!.close(3)
  const second = callback({ toolName: "write", input: { path: "new.rs" }, content: [] })
  spawned[1]!.stdout.write(JSON.stringify(pass))
  spawned[1]!.close()
  const result = await first as { content: Array<{ text: string }> }
  assert.equal(result.content[0]!.text, "old edit")
  assert.match(result.content.at(-1)!.text, /old completed failure/)
  assert.match(result.content.at(-1)!.text, /superseded by a newer edit/)
  assert.equal(await second, undefined)
  assert.deepEqual(spawned[0]!.signals, [])
})

test("D7 policy deletion cancels a pending run before checking config presence", async (t) => {
  const spawned = children(t)
  const root = project(t)
  const callback = extension(t, root)
  const first = callback({ toolName: "write", input: { path: "old.rs" }, content: [] })
  rmSync(join(root, ".ironlint.yml"))
  assert.equal(await callback({ toolName: "delete", input: { path: ".ironlint.yml" }, content: [] }), undefined)
  assert.equal(spawned[0]!.stdin.writableEnded, true)
  assert.deepEqual(spawned[0]!.signals, [])
  assert.equal(spawned.length, 1)
  spawned[0]!.close(2)
  assert.equal(await first, undefined)
})

test("D7 cancellation without closure reports cleanup failure but appends no stale result", async (t) => {
  const spawned = children(t)
  const warnings = t.mock.method(console, "error", () => {})
  t.mock.timers.enable({ apis: ["setTimeout"] })
  const callback = extension(t)
  const first = callback({ toolName: "write", input: { path: "old.rs" }, content: [] })
  const second = callback({ toolName: "write", input: { path: "new.rs" }, content: [] })
  spawned[1]!.stdout.write(JSON.stringify(pass))
  spawned[1]!.close()
  t.mock.timers.tick(GRACE_MS)
  assert.equal(await first, undefined)
  assert.equal(await second, undefined)
  assert.equal(warnings.mock.callCount(), 1)
  assert.match(String(warnings.mock.calls[0]!.arguments[0]), /closure.*not observed/)
  spawned[0]!.emit("error", new Error("late old error"))
  t.mock.timers.tick(WALL_MS + GRACE_MS)
  assert.equal(warnings.mock.callCount(), 1)
})

test("D7 failed mutation and known-empty batch do not cancel active feedback", async (t) => {
  const spawned = children(t)
  const callback = extension(t)
  const pending = callback({ toolName: "write", input: { path: "a.rs" }, content: [] })
  await callback({ toolName: "write", isError: true, input: { path: "b.rs" }, content: [] })
  await callback({ toolName: "batch", input: { paths: [] }, content: [] })
  assert.equal(spawned.length, 1)
  assert.deepEqual(spawned[0]!.signals, [])
  spawned[0]!.stdout.write(JSON.stringify(pass))
  spawned[0]!.close()
  assert.equal(await pending, undefined)
})

test("D7 synchronous spawn error becomes an incomplete result", async (t) => {
  children(t)
  t.mock.method(childProcess, "spawn", () => { throw new Error("bad spawn options") })
  syncBuiltinESMExports()
  const result = await runIronLint([], "/tmp")
  assert.equal(result.exitCode, 3)
  assert.match(result.stderr, /bad spawn options/)
})

test("D7 failed process-group cleanup is reported even when child closes", async (t) => {
  if (process.platform === "win32") return
  const spawned = children(t)
  t.mock.method(process, "kill", () => { throw Object.assign(new Error("denied"), { code: "EPERM" }) })
  t.mock.timers.enable({ apis: ["setTimeout"] })
  const pending = runIronLint([], "/tmp")
  t.mock.timers.tick(WALL_MS)
  spawned[0]!.onKill = () => spawned[0]!.close()
  t.mock.timers.tick(GRACE_MS)
  spawned[0]!.close()
  assert.match((await pending).stderr, /process-group cleanup failed: denied/)
})

test("D7 kill error emitted while stopping remains a cleanup diagnostic", async (t) => {
  const spawned = children(t)
  t.mock.timers.enable({ apis: ["setTimeout"] })
  const pending = runIronLint([], "/tmp")
  spawned[0]!.killError = new Error("direct kill denied")
  t.mock.timers.tick(WALL_MS)
  spawned[0]!.onKill = () => spawned[0]!.close()
  t.mock.timers.tick(GRACE_MS)
  spawned[0]!.close()
  assert.match((await pending).stderr, /cleanup.*direct kill denied/)
})

test("D7 unconfirmed child closure releases the host event-loop reference", async (t) => {
  const spawned = children(t)
  t.mock.timers.enable({ apis: ["setTimeout"] })
  const pending = runIronLint([], "/tmp")
  t.mock.timers.tick(WALL_MS)
  t.mock.timers.tick(GRACE_MS)
  await pending
  assert.equal(spawned[0]!.unreferenced, true)
})

test("D7 malformed UTF-8 stdout cannot become a silently repaired pass", async (t) => {
  const spawned = children(t)
  const pending = runIronLint([], "/tmp")
  const json = JSON.stringify({ ...pass, results: [{ ...passResult, id: "MARKER" }] })
  const marker = json.indexOf("MARKER")
  spawned[0]!.stdout.write(Buffer.concat([Buffer.from(json.slice(0, marker)), Buffer.from([0xff]), Buffer.from(json.slice(marker + 6))]))
  spawned[0]!.close()
  const result = await pending
  assert.equal(result.exitCode, 3)
  assert.equal(result.stdout, "")
  assert.match(result.stderr, /incomplete.*UTF-8/)
})
