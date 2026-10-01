import { test, type TestContext } from "node:test"
import assert from "node:assert/strict"
import childProcess, { spawnSync, type ChildProcess } from "node:child_process"
import { syncBuiltinESMExports } from "node:module"
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { delimiter, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import ironlintExtension, { runIronLint } from "../src/index.ts"

function isolated(t: TestContext): string {
  const root = mkdtempSync(join(tmpdir(), "ironlint-pi-process-"))
  const original = { PATH: process.env.PATH, HOME: process.env.HOME, XDG_CONFIG_HOME: process.env.XDG_CONFIG_HOME }
  process.env.HOME = root
  process.env.XDG_CONFIG_HOME = join(root, ".config")
  t.after(() => {
    for (const [key, value] of Object.entries(original)) {
      if (value === undefined) delete process.env[key]
      else process.env[key] = value
    }
    rmSync(root, { recursive: true, force: true })
  })
  return root
}

function callback(root: string): (event: unknown) => Promise<unknown> {
  let handler: ((event: unknown) => Promise<unknown>) | undefined
  ironlintExtension({ cwd: root, on(_, value) { handler = value as typeof handler } })
  assert.ok(handler)
  return handler
}

async function waitFor(condition: () => boolean): Promise<void> {
  const until = Date.now() + 2_000
  while (!condition() && Date.now() < until) await new Promise((resolve) => setTimeout(resolve, 10))
  assert.ok(condition(), "owned process did not reach the expected state within two seconds")
}

test("D7 missing evaluator reports spawn failure in an isolated PATH", async (t) => {
  const root = isolated(t)
  const bin = join(root, "empty-bin")
  mkdirSync(bin)
  process.env.PATH = bin
  const result = await runIronLint([], root)
  assert.equal(result.exitCode, 3)
  assert.match(result.stderr, /ENOENT/)
})

test("D7 ordinary cancellation reaps the owned child and closes descendant pipes", async (t) => {
  if (process.platform === "win32") return t.skip("Unix process-group smoke")
  const root = isolated(t)
  const bin = join(root, "bin")
  mkdirSync(bin)
  process.env.PATH = bin + delimiter + (process.env.PATH ?? "")
  const marker = join(root, "owned-pids.json")
  const script = join(root, "hang.cjs")
  writeFileSync(script, `
const { spawn } = require("node:child_process")
const { writeFileSync } = require("node:fs")
const descendant = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: ["ignore", "inherit", "inherit"] })
writeFileSync(${JSON.stringify(marker)}, JSON.stringify([process.pid, descendant.pid]))
setInterval(() => process.stdout.write("pending bytes"), 10)
`)
  const executable = join(bin, "ironlint")
  const quote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`
  writeFileSync(executable, `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)}\n`)
  chmodSync(executable, 0o755)
  writeFileSync(join(root, ".ironlint.yml"), "version: 1\nchecks: {}\n")
  const originalSpawn = childProcess.spawn
  let owned: ChildProcess | undefined
  t.mock.method(childProcess, "spawn", (...args: Parameters<typeof childProcess.spawn>) => {
    owned = originalSpawn(...args)
    return owned
  })
  syncBuiltinESMExports()
  t.after(() => {
    if (owned?.pid && owned.exitCode === null && owned.signalCode === null) {
      try { process.kill(-owned.pid, "SIGKILL") } catch {}
      owned.kill("SIGKILL")
    }
    t.mock.restoreAll()
    syncBuiltinESMExports()
  })
  const handler = callback(root)
  const pending = handler({ toolName: "write", input: { path: "first.rs" }, content: [] })
  await waitFor(() => existsSync(marker))
  const [pid, descendant] = JSON.parse(readFileSync(marker, "utf8")) as number[]
  assert.equal(pid, owned!.pid)
  rmSync(join(root, ".ironlint.yml"))
  await handler({ toolName: "delete", input: { path: ".ironlint.yml" }, content: [] })
  assert.equal(await pending, undefined)
  await waitFor(() => owned!.signalCode === "SIGKILL")
  assert.equal(owned!.signalCode, "SIGKILL")
  assert.throws(() => process.kill(pid!, 0), { code: "ESRCH" })
  await waitFor(() => {
    try { process.kill(descendant!, 0); return false } catch (error) {
      assert.equal((error as NodeJS.ErrnoException).code, "ESRCH")
      return true
    }
  })
  assert.equal(owned!.stdout!.destroyed, true)
  assert.equal(owned!.stderr!.destroyed, true)
  assert.equal(owned!.listenerCount("close"), 0)
})

test("D7 fresh CLI produces accepted complete change feedback with isolated consent", async (t) => {
  const binary = process.env.IRONLINT_TEST_BIN ?? resolve(fileURLToPath(new URL("../../../target/debug/ironlint", import.meta.url)))
  if (!existsSync(binary)) return t.skip("build ironlint before the real CLI smoke")
  const root = isolated(t)
  process.env.PATH = resolve(binary, "..") + delimiter + (process.env.PATH ?? "")
  const config = join(root, ".ironlint.yml")
  writeFileSync(config, "version: 1\nchecks:\n  fast: {on: [change, accept], run: 'printf bounded'}\n")
  const trusted = spawnSync(binary, ["trust", "--config", config], { cwd: root, encoding: "utf8" })
  assert.equal(trusted.status, 0, trusted.stderr)
  const handler = callback(root)
  assert.equal(await handler({ toolName: "write", input: { path: "a.rs" }, content: [] }), undefined)
  writeFileSync(config, "version: 1\nchecks:\n  fast: {on: [change, accept], run: 'exit 2'}\n")
  assert.equal(spawnSync(binary, ["trust", "--config", config], { cwd: root }).status, 0)
  const result = await handler({ toolName: "write", input: { path: "a.rs" }, content: [{ type: "text", text: "written" }] }) as { content: Array<{ text: string }> }
  assert.equal(result.content[0]!.text, "written")
  assert.match(result.content.at(-1)!.text, /IronLint change feedback: fast:/)
})

test("D7 real CLI cancellation stops a check in its independent process group", async (t) => {
  if (process.platform === "win32") return t.skip("Unix process-group cancellation")
  const binary = process.env.IRONLINT_TEST_BIN ?? resolve(fileURLToPath(new URL("../../../target/debug/ironlint", import.meta.url)))
  if (!existsSync(binary)) return t.skip("build ironlint before the real CLI smoke")
  const root = isolated(t)
  process.env.PATH = resolve(binary, "..") + delimiter + (process.env.PATH ?? "")
  const config = join(root, ".ironlint.yml")
  const marker = join(root, "owned-check.pid")
  let pid: number | undefined
  t.after(() => {
    if (pid) {
      try { process.kill(-pid, "SIGKILL") } catch {}
      try { process.kill(pid, "SIGKILL") } catch {}
    }
  })
  writeFileSync(config, `version: 1
execution: {timeout_secs: 60, total_timeout_secs: 60}
checks:
  slow:
    on: [change, accept]
    run: 'printf "%s\\n" "$$" > owned-check.pid; exec sleep 30'
`)
  assert.equal(spawnSync(binary, ["trust", "--config", config], { cwd: root }).status, 0)
  const handler = callback(root)
  const pending = handler({ toolName: "write", input: { path: "a.rs" }, content: [] })
  await waitFor(() => existsSync(marker) && readFileSync(marker, "utf8").trim().length > 0)
  pid = Number(readFileSync(marker, "utf8").trim())
  assert.ok(Number.isInteger(pid) && pid > 0)
  rmSync(config)
  await handler({ toolName: "delete", input: { path: ".ironlint.yml" }, content: [] })
  assert.equal(await pending, undefined)
  await new Promise((resolve) => setTimeout(resolve, 250))
  assert.throws(() => process.kill(pid!, 0), { code: "ESRCH" })
})
