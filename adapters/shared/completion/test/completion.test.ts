import { test, type TestContext } from "node:test"
import assert from "node:assert/strict"
import { execFileSync } from "node:child_process"
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { fileURLToPath } from "node:url"
import { runControlled, type CompletionOptions, type ControlledHost } from "../completion.ts"
import { runOwned } from "../process.ts"

function project(t: TestContext, run: ControlledHost["run"]): CompletionOptions {
  const base = mkdtempSync(join(tmpdir(), "ironlint-native-completion-test-"))
  t.after(() => rmSync(base, { recursive: true, force: true }))
  const root = join(base, "work")
  const artifacts = join(base, "artifacts")
  mkdirSync(root); mkdirSync(artifacts)
  execFileSync("git", ["init", "-q", root])
  const binary = join(base, "ironlint")
  const policy = join(base, "policy.yml")
  writeFileSync(policy, "version: 1\nchecks:\n  test:\n    run: test -f pass.txt\n    on: [accept]\n")
  const result = { id: "test", outcome: "pass", exit_status: 0, stdout: [], stderr: [], stdout_truncated: false, stderr_truncated: false, reason: null }
  writeFileSync(binary, `#!/usr/bin/env node\nconst fs=require('fs');const args=process.argv.slice(2);const root=args[args.indexOf('--root')+1];const pass=fs.existsSync(root+'/pass.txt');const result=${JSON.stringify(result)};if(!pass){result.outcome='violation';result.exit_status=1;}process.stdout.write(JSON.stringify({schema:7,event:'accept',error:null,not_run:[],status:pass?'pass':'violation',results:[result]}));process.exitCode=pass?0:2;\n`, { mode: 0o755 })
  return { root, artifacts, binary, policy, expectedIds: ["test"], prompt: "finish", host: { async validate() {}, run } }
}

test("persistent violations end incomplete within repair limit even when model says done", async (t) => {
  let calls = 0
  const options = project(t, async () => { calls++; return "done" })
  const result = await runControlled({ ...options, maxRepairTurns: 2 })
  assert.equal(result.status, "incomplete")
  assert.equal(result.repairTurns, 2)
  assert.equal(calls, 3)
  assert.equal(result.modelText, undefined)
  assert.equal(result.candidate, undefined)
  assert.equal(readdirSync(join(options.root, ".git")).some((path) => path.startsWith("ironlint-completion-")), false)
})

test("repair passes only a fresh full captured candidate and leaves index unchanged", async (t) => {
  let calls = 0
  const options = project(t, async (_prompt, root) => { if (++calls === 2) writeFileSync(join(root, "pass.txt"), "accepted bytes"); return "complete model text" })
  writeFileSync(join(options.root, "dirty.txt"), "staged")
  execFileSync("git", ["add", "dirty.txt"], { cwd: options.root })
  writeFileSync(join(options.root, "dirty.txt"), "working")
  const index = readFileSync(join(options.root, ".git", "index"))
  const result = await runControlled(options)
  assert.equal(result.status, "complete", result.diagnostics)
  assert.equal(result.repairTurns, 1)
  assert.equal(result.modelText, "complete model text")
  assert.equal(readFileSync(join(result.candidate!.tree, "dirty.txt"), "utf8"), "working")
  assert.deepEqual(readFileSync(join(options.root, ".git", "index")), index)
  result.candidate!.dispose()
  const fresh = await runControlled(options)
  assert.equal(fresh.status, "complete", fresh.diagnostics)
  assert.equal(fresh.repairTurns, 0)
  fresh.candidate!.dispose()
})

test("same canonical source is exclusive across different artifact stores", async (t) => {
  let release!: () => void
  let entered!: () => void
  const started = new Promise<void>((resolve) => { entered = resolve })
  const options = project(t, async (_prompt, root) => { entered(); await new Promise<void>((resolve) => { release = resolve }); writeFileSync(join(root, "pass.txt"), "yes"); return "done" })
  const first = runControlled(options)
  await started
  const artifacts = join(options.artifacts, "different")
  mkdirSync(artifacts)
  const second = await runControlled({ ...options, artifacts })
  assert.equal(second.status, "incomplete")
  assert.match(second.diagnostics, /concurrent|lock/i)
  release()
  const result = await first
  assert.equal(result.status, "complete", result.diagnostics)
  result.candidate!.dispose()
})

test("infrastructure failures, missing IDs, and artifact mutation terminate without repairs", async (t) => {
  for (const mode of ["missing", "ids", "mutation"]) {
    const options = project(t, async (_prompt, root) => { writeFileSync(join(root, "pass.txt"), "yes"); return "done" })
    if (mode === "missing") rmSync(options.binary)
    if (mode === "ids") options.expectedIds = ["different"]
    if (mode === "mutation") writeFileSync(options.binary, "#!/bin/sh\necho mutation > changed.txt\nprintf '%s' '{\"schema\":7,\"event\":\"accept\",\"error\":null,\"not_run\":[],\"status\":\"pass\",\"results\":[{\"id\":\"test\",\"outcome\":\"pass\",\"exit_status\":0,\"stdout\":[],\"stderr\":[],\"stdout_truncated\":false,\"stderr_truncated\":false,\"reason\":null}]}'\n")
    const result = await runControlled(options)
    assert.equal(result.status, "incomplete", mode)
    assert.equal(result.repairTurns, 0)
  }
})

test("owner policy drift and caller cancellation discard a provisional success", async (t) => {
  const controller = new AbortController()
  const cancelled = project(t, async (_prompt, root) => { writeFileSync(join(root, "pass.txt"), "yes"); controller.abort(); return "done" })
  const cancellation = await runControlled({ ...cancelled, signal: controller.signal })
  assert.equal(cancellation.status, "incomplete")
  assert.match(cancellation.diagnostics, /cancel/i)
  assert.equal(cancellation.candidate, undefined)
  const changed = project(t, async (_prompt, root) => { writeFileSync(join(root, "pass.txt"), "yes"); writeFileSync(changed.policy, "weakened"); return "done" })
  const drift = await runControlled(changed)
  assert.equal(drift.status, "incomplete")
  assert.match(drift.diagnostics, /owner.*changed/i)
})

test("model work consumes the same injected total deadline as capture and checks", async (t) => {
  let now = 0
  const options = project(t, async (_prompt, root) => { writeFileSync(join(root, "pass.txt"), "yes"); now += 7_001; return "done" })
  const result = await runControlled({ ...options, deadlineMs: 9_000, now: () => now })
  assert.equal(result.status, "incomplete")
  assert.match(result.diagnostics, /deadline/i)
  assert.equal(result.candidate, undefined)
})

test("late source drift causes a fresh bounded recapture rather than stale completion", async (t) => {
  const options = project(t, async (_prompt, root) => { writeFileSync(join(root, "pass.txt"), "yes"); return "done" })
  const binary = readFileSync(options.binary, "utf8")
  writeFileSync(options.binary, binary.replace("const result=", `if(!fs.existsSync(${JSON.stringify(join(options.root, "late.txt"))})){fs.writeFileSync(${JSON.stringify(join(options.root, "late.txt"))},'late mutation')}const result=`))
  const result = await runControlled(options)
  assert.equal(result.status, "complete", result.diagnostics)
  assert.equal(readFileSync(join(result.candidate!.tree, "late.txt"), "utf8"), "late mutation")
  result.candidate!.dispose()
  writeFileSync(options.binary, binary.replace("const result=", `fs.writeFileSync(${JSON.stringify(join(options.root, "late.txt"))},String(Math.random()));const result=`))
  const unstable = await runControlled(options)
  assert.equal(unstable.status, "incomplete")
  assert.match(unstable.diagnostics, /kept changing/i)
})

test("real IronLint acceptance denies and repairs under isolated owner consent", async (t) => {
  const binary = process.env.IRONLINT_TEST_BIN ?? fileURLToPath(new URL("../../../../target/debug/ironlint", import.meta.url))
  if (!existsSync(binary)) return t.skip("build ironlint and set IRONLINT_TEST_BIN")
  let calls = 0
  const options = project(t, async (_prompt, root) => { if (++calls === 2) writeFileSync(join(root, "pass.txt"), "yes"); return "done" })
  const configHome = join(options.artifacts, "consent")
  mkdirSync(configHome)
  const previous = process.env.XDG_CONFIG_HOME
  process.env.XDG_CONFIG_HOME = configHome
  t.after(() => { if (previous === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previous })
  execFileSync(binary, ["trust", "--config", options.policy], { cwd: options.root, env: { ...process.env, XDG_CONFIG_HOME: configHome } })
  const result = await runControlled({ ...options, binary })
  assert.equal(result.status, "complete", result.diagnostics)
  assert.equal(result.repairTurns, 1)
  result.candidate!.dispose()
  writeFileSync(options.policy, "version: 1\nchecks:\n  test:\n    run: 'true'\n")
  const revoked = await runControlled({ ...options, binary })
  assert.equal(revoked.status, "incomplete")
  assert.equal(revoked.repairTurns, 0)
})

test("unconfirmed descendant settlement retains the workspace lock until its process group disappears", async (t) => {
  let pidFile = ""
  const options = project(t, async (_prompt, root, deadline) => {
    await runOwned({ executable: process.execPath, root, deadline,
      args: ["-e", `require('fs').writeFileSync(${JSON.stringify(pidFile)},String(process.pid));require('child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore'}).unref()`],
    })
    return "done"
  })
  pidFile = join(options.artifacts, "group")
  const originalKill = process.kill
  const delayed = new Set<number>()
  process.kill = ((pid: number, signal?: number | NodeJS.Signals) => {
    if (pid < 0 && signal === "SIGKILL") {
      if (!delayed.has(pid)) {
        delayed.add(pid)
        setTimeout(() => { try { originalKill(pid, "SIGKILL") } catch {} }, 2_500)
      }
      return true
    }
    return originalKill(pid, signal)
  }) as typeof process.kill
  t.after(() => { process.kill = originalKill })
  const result = await runControlled(options)
  assert.equal(result.status, "incomplete")
  assert.match(result.diagnostics, /cleanup unconfirmed/i)
  const locks = () => readdirSync(join(options.root, ".git")).filter((path) => path.startsWith("ironlint-completion-"))
  assert.equal(locks().length, 1)
  const second = await runControlled(options)
  assert.equal(second.status, "incomplete")
  assert.match(second.diagnostics, /concurrent|lock/i)
  const until = Date.now() + 2_000
  while (locks().length && Date.now() < until) await new Promise((resolve) => setTimeout(resolve, 20))
  assert.equal(locks().length, 0)
  const group = -Number(readFileSync(pidFile, "utf8"))
  assert.throws(() => originalKill(group, 0), /ESRCH|No such process/i)
})
