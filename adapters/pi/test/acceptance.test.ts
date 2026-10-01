import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { classifyAcceptance } from "../src/acceptance.ts"

const result = (id: string, outcome = "pass", exit_status = 0) => ({
  id, outcome, exit_status, reason: null, stdout: [], stderr: [],
  stdout_truncated: false, stderr_truncated: false,
})
const pass = { schema: 7, event: "accept", status: "pass", error: null, not_run: [], results: [result("fmt"), result("test")] }
const verdict = (value: unknown) => Buffer.from(JSON.stringify(value))
const classify = (value: unknown, code = 0) => classifyAcceptance(verdict(value), code, ["fmt", "test"])

test("only a complete exact passing acceptance verdict permits completion", () => {
  assert.equal(classify(pass).kind, "pass")
  for (const invalid of [
    { ...pass, schema: 6 }, { ...pass, event: "change" }, { ...pass, status: "not_run" },
    { ...pass, error: "oops" }, { ...pass, not_run: [{ id: "test", reason: "timeout" }] },
    { ...pass, results: [result("fmt")] }, { ...pass, results: [result("fmt"), result("fmt")] },
    { ...pass, results: [result("fmt"), result("extra")] },
    { ...pass, results: [result("fmt", "pass", 1), result("test")] },
    { ...pass, results: [result("fmt", "error"), result("test")] },
  ]) assert.notEqual(classify(invalid).kind, "pass")
  assert.notEqual(classify(pass, 3).kind, "pass")
  assert.notEqual(classifyAcceptance(Buffer.concat([verdict(pass), Buffer.from("\n"), verdict(pass)]), 0, ["fmt", "test"]).kind, "pass")
  assert.notEqual(classifyAcceptance(Buffer.from([0xff]), 0, ["fmt", "test"]).kind, "pass")
})

test("a valid violation is repairable; evaluator errors stay incomplete", () => {
  const violation = { ...pass, status: "violation", results: [result("fmt"), { ...result("test", "violation", 1), stdout: [...Buffer.from("line 4: fix test")] }] }
  assert.deepEqual(classify(violation, 2), { kind: "violation", diagnostics: "test: line 4: fix test" })
  assert.equal(classify(violation, 3).kind, "incomplete")
  assert.equal(classify({ ...violation, not_run: [{ id: "later", reason: "timeout" }] }, 2).kind, "incomplete")
  assert.equal(classify({ ...violation, results: [result("fmt"), result("test", "error", 127)] }, 3).kind, "incomplete")
})

test("shared shell and Pi acceptance conformance cases", () => {
  const fixture = JSON.parse(readFileSync(fileURLToPath(new URL("../../../tests/fixtures/acceptance-conformance.json", import.meta.url)), "utf8")) as {
    expectedIds: string[]; cases: Array<{ name: string; kind: string; exit: number; verdict: unknown }>
  }
  for (const item of fixture.cases) {
    assert.equal(classifyAcceptance(verdict(item.verdict), item.exit, fixture.expectedIds).kind, item.kind, item.name)
  }
})
