import { test } from "node:test"
import assert from "node:assert/strict"
import { mkdtempSync, readFileSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { Deadline } from "../deadline.ts"
import { CleanupUnconfirmedError, runOwned } from "../process.ts"

test("owned child output is returned only after settlement", async () => {
  const deadline = new Deadline(10_000)
  try {
    const run = await runOwned({ executable: process.execPath, args: ["-e", "process.stdout.write('done');process.stderr.write('detail')"], root: process.cwd(), deadline })
    assert.equal(run.code, 0)
    assert.equal(run.stdout.toString(), "done")
    assert.equal(run.stderr.toString(), "detail")
  } finally { deadline.dispose() }
})

test("stdout overflow, cancellation, and background descendants never succeed", async () => {
  const overflow = new Deadline(10_000)
  try {
    await assert.rejects(runOwned({ executable: process.execPath, args: ["-e", "process.stdout.write(Buffer.alloc(8*1024*1024+1))"], root: process.cwd(), deadline: overflow }), /stdout.*limit/i)
  } finally { overflow.dispose() }
  const controller = new AbortController()
  const cancelled = new Deadline(10_000, controller.signal)
  const pending = runOwned({ executable: process.execPath, args: ["-e", "setInterval(()=>{},1000)"], root: process.cwd(), deadline: cancelled })
  controller.abort()
  await assert.rejects(pending, /cancel/i)
  cancelled.dispose()
  const background = new Deadline(10_000)
  try {
    await assert.rejects(runOwned({ executable: process.execPath, args: ["-e", "require('child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore'}).unref()"], root: process.cwd(), deadline: background }), /background/i)
  } finally { background.dispose() }
})

test("leader exit with inherited descendant pipes is cancelled and cleaned within the total budget", async () => {
  const deadline = new Deadline(2_300)
  const started = Date.now()
  try {
    await assert.rejects(runOwned({ executable: process.execPath,
      args: ["-e", "require('child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:['ignore',1,2]}).unref()"],
      root: process.cwd(), deadline,
    }), /deadline|time limit/i)
    assert.ok(Date.now() - started < 2_500)
  } finally { deadline.dispose() }
})

test("closed-pipe background ownership waits for the process group, not just the leader", async (t) => {
  const directory = mkdtempSync(join(tmpdir(), "ironlint-background-settlement-"))
  const pidFile = join(directory, "group")
  const deadline = new Deadline(10_000)
  const originalKill = process.kill
  const delayed = new Set<number>()
  // Exercise a real descendant with delayed OS signal delivery. Reaping the
  // already-exited leader cannot prove this descendant has stopped writing.
  process.kill = ((pid: number, signal?: number | NodeJS.Signals) => {
    if (pid < 0 && signal === "SIGKILL") {
      if (!delayed.has(pid)) {
        delayed.add(pid)
        setTimeout(() => { try { originalKill(pid, "SIGKILL") } catch {} }, 150)
      }
      return true
    }
    return originalKill(pid, signal)
  }) as typeof process.kill
  t.after(() => { process.kill = originalKill; deadline.dispose(); rmSync(directory, { recursive: true, force: true }) })
  let failure: unknown
  try {
    await runOwned({ executable: process.execPath,
      args: ["-e", `require('fs').writeFileSync(${JSON.stringify(pidFile)},String(process.pid));require('child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore'}).unref()`],
      root: process.cwd(), deadline,
    })
  } catch (error) { failure = error }
  assert.match((failure as Error).message, /background/i)
  const group = -Number(readFileSync(pidFile, "utf8"))
  let exists = false
  try { originalKill(group, 0); exists = true } catch {}
  if (exists) {
    assert.ok(failure instanceof CleanupUnconfirmedError, "live group must retain ownership until settlement")
    let settled = false
    void failure.settled.then(() => { settled = true })
    await Promise.resolve()
    assert.equal(settled, false)
  }
})
