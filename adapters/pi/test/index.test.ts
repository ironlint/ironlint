import { after, test } from "node:test"
import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import { chmodSync, existsSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { delimiter, join } from "node:path"
import { changedPaths, feedback } from "../src/index.ts"
import ironlintExtension from "../src/index.ts"

type Handler = (event: unknown) => Promise<unknown>

function loadExtension(root: string): Record<string, Handler> {
  const handlers: Record<string, Handler> = {}
  ironlintExtension({
    cwd: root,
    on(event, handler) {
      handlers[event] = handler as Handler
    },
  })
  return handlers
}

const temp = mkdtempSync(join(tmpdir(), "ironlint-pi-v1-"))
const bin = join(temp, "bin")
mkdirSync(bin)
const originalPath = process.env.PATH ?? ""
process.env.PATH = bin + delimiter + originalPath
after(() => {
  process.env.PATH = originalPath
  rmSync(temp, { recursive: true, force: true })
})

function fakeIronLint(exitCode: number, stdout = "", stderr = "", delay = ""): void {
  const file = join(bin, "ironlint")
  writeFileSync(
    file,
    `#!/bin/sh
${delay}
printf '%s' '${stdout.replace(/'/g, "'\\\"'\\\"'")}'
printf '%s' '${stderr.replace(/'/g, "'\\\"'\\\"'")}' >&2
exit ${exitCode}
`,
  )
  chmodSync(file, 0o755)
}

function project(): string {
  const root = mkdtempSync(join(tmpdir(), "ironlint-pi-project-"))
  writeFileSync(join(root, ".ironlint.yml"), "version: 1\nchecks: {}\n")
  return root
}

const violation = JSON.stringify({
  results: [{ id: "no-panic", outcome: "violation", reason: "panic found" }],
})

test("changedPaths includes delete paths and both rename endpoints", () => {
  assert.deepEqual(changedPaths("delete", { path: "src/dead.rs" }), ["src/dead.rs"])
  assert.deepEqual(changedPaths("rename", { oldPath: "src/old.rs", newPath: "src/new.rs" }), [
    "src/old.rs",
    "src/new.rs",
  ])
})

test("feedback names the failed check and diagnostic", () => {
  assert.equal(feedback(violation, "fallback"), "no-panic: panic found")
})

test("tool_result keeps a failing write, reports a violation, and gives a reproduction command", async () => {
  fakeIronLint(2, violation)
  const root = project()
  try {
    const result = (await loadExtension(root).tool_result!({
      toolName: "write",
      input: { path: "src/main.rs" },
      content: [{ type: "text", text: "Successfully wrote src/main.rs" }],
      isError: false,
    })) as { content?: Array<{ text?: string }> }
    const text = result.content?.at(-1)?.text ?? ""
    assert.match(text, /no-panic: panic found/)
    assert.match(text, /Reproduce: ironlint check --event change/)
    assert.match(text, /--file src\/main.rs/)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test("reproduction command does not evaluate shell syntax in a path", async () => {
  fakeIronLint(2, violation)
  const root = project()
  const marker = join(root, "owned")
  try {
    const result = (await loadExtension(root).tool_result!({
      toolName: "write",
      input: { path: `src/it's-$(touch ${marker}).rs` },
      content: [],
      isError: false,
    })) as { content?: Array<{ text?: string }> }
    const text = result.content?.at(-1)?.text ?? ""
    const command = text.split("\nReproduce: ")[1]?.split("\n")[0] ?? ""

    assert.notEqual(command, "")
    assert.equal(spawnSync("sh", ["-c", command], { cwd: root }).status, 2)
    assert.equal(existsSync(marker), false)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test("tool_result includes both rename endpoints", async () => {
  fakeIronLint(2, violation)
  const root = project()
  try {
    const result = (await loadExtension(root).tool_result!({
      toolName: "rename",
      input: { oldPath: "src/old.rs", newPath: "src/new.rs" },
      content: [],
      isError: false,
    })) as { content?: Array<{ text?: string }> }
    const text = result.content?.at(-1)?.text ?? ""
    assert.match(text, /--file src\/old.rs --file src\/new.rs/)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test("tool_result reports evaluator failures without vetoing a later edit", async () => {
  fakeIronLint(3, "", "deadline exceeded")
  const root = project()
  try {
    const result = (await loadExtension(root).tool_result!({
      toolName: "edit",
      input: { path: "src/main.rs" },
      content: [],
      isError: false,
    })) as { content?: Array<{ text?: string }>; isError?: boolean }
    assert.match(result.content?.at(-1)?.text ?? "", /could not evaluate this change: deadline exceeded/)
    assert.equal(result.isError, undefined)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test("tool_result suppresses successful feedback and ignores failed writes", async () => {
  fakeIronLint(0)
  const root = project()
  try {
    const handlers = loadExtension(root)
    assert.equal(
      await handlers.tool_result!({ toolName: "write", input: { path: "src/a.rs" }, content: [], isError: false }),
      undefined,
    )
    assert.equal(
      await handlers.tool_result!({ toolName: "write", input: { path: "src/a.rs" }, content: [], isError: true }),
      undefined,
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test("tool_result skips a known-empty batch", async () => {
  fakeIronLint(2, violation)
  const root = project()
  try {
    assert.equal(
      await loadExtension(root).tool_result!({ toolName: "batch", input: { paths: [] }, content: [], isError: false }),
      undefined,
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test("tool_result labels feedback superseded when a newer edit arrives", async () => {
  fakeIronLint(2, violation, "", "sleep 0.1")
  const root = project()
  try {
    const handler = loadExtension(root).tool_result!
    const first = handler({ toolName: "write", input: { path: "src/first.rs" }, content: [], isError: false })
    const second = handler({ toolName: "write", input: { path: "src/second.rs" }, content: [], isError: false })
    const firstResult = (await first) as { content?: Array<{ text?: string }> }
    await second
    assert.match(firstResult.content?.at(-1)?.text ?? "", /superseded by a newer edit/)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})

test("tool_result supersedes pending feedback when a newer mutation deletes the policy", async () => {
  fakeIronLint(2, violation, "", "sleep 0.1")
  const root = project()
  try {
    const handler = loadExtension(root).tool_result!
    const first = handler({ toolName: "write", input: { path: "src/first.rs" }, content: [], isError: false })
    rmSync(join(root, ".ironlint.yml"))
    await handler({ toolName: "delete", input: { path: ".ironlint.yml" }, content: [], isError: false })

    const firstResult = (await first) as { content?: Array<{ text?: string }> }
    assert.match(firstResult.content?.at(-1)?.text ?? "", /superseded by a newer edit/)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})
