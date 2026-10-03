import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import { fileURLToPath } from "node:url"
import test from "node:test"

const cli = fileURLToPath(new URL("../cli.ts", import.meta.url))
test("invalid or missing owner inputs yield a single incomplete terminal result", () => {
  for (const args of [[], ["unknown"], ["codex", "--root"], ["claude-code", "--force-pass"],
    ["codex", "--root", "/tmp", "--root", "/tmp"]]) {
    const child = spawnSync(process.execPath, ["--experimental-strip-types", cli, ...args], { encoding: "utf8" })
    assert.equal(child.status, 3, child.stderr)
    const lines = child.stdout.trim().split("\n")
    assert.equal(lines.length, 1)
    const result = JSON.parse(lines[0])
    assert.equal(result.status, "incomplete")
    assert.equal(result.repairTurns, 0)
    assert.equal(typeof result.diagnostics, "string")
    assert.equal(result.candidate, undefined)
    assert.equal(result.modelText, undefined)
  }
})
