import { test } from "node:test"
import assert from "node:assert/strict"
import { mkdtempSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { confined, createPiSession, ownerModelSettings } from "../src/sdk.ts"

test("controlled Pi tools cannot target owner paths, .git, or symlink escapes", () => {
  const base = mkdtempSync(join(tmpdir(), "ironlint-pi-tools-"))
  try {
    const root = join(base, "work")
    mkdirSync(root)
    symlinkSync(base, join(root, "out"))
    assert.equal(confined(root, join(root, "file.txt")), join(root, "file.txt"))
    assert.equal(confined(root, root, true), root)
    assert.throws(() => confined(root, join(base, "owner.yml")), /outside/)
    assert.throws(() => confined(root, join(root, "..", "owner.yml")), /outside/)
    assert.throws(() => confined(root, join(root, ".git", "index")), /outside/)
    assert.throws(() => confined(root, join(root, ".GIT", "HEAD")), /outside/)
    assert.throws(() => confined(root, join(root, "out", "owner.yml")), /symlink/)
  } finally { rmSync(base, { recursive: true, force: true }) }
})

test("pinned Pi SDK starts with isolated agent state and reports initialization failure", async () => {
  const base = mkdtempSync(join(tmpdir(), "ironlint-pi-startup-"))
  try {
    const root = join(base, "work")
    const agent = join(base, "agent")
    mkdirSync(root)
    mkdirSync(agent)
    const session = await createPiSession(root, agent)
    assert.equal(session.isIdle, true)
    session.dispose()
    await assert.rejects(createPiSession(join(base, "missing"), agent))
  } finally { rmSync(base, { recursive: true, force: true }) }
})

test("controlled session selects only owner global model defaults", () => {
  const base = mkdtempSync(join(tmpdir(), "ironlint-pi-settings-"))
  try {
    writeFileSync(join(base, "settings.json"), JSON.stringify({ defaultProvider: "openrouter", defaultModel: "safe/model", packages: ["ignored"] }))
    assert.deepEqual(ownerModelSettings(base), { defaultProvider: "openrouter", defaultModel: "safe/model" })
  } finally { rmSync(base, { recursive: true, force: true }) }
})
