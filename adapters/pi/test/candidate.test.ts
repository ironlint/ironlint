import { test, type TestContext } from "node:test"
import assert from "node:assert/strict"
import { execFileSync } from "node:child_process"
import { chmodSync, existsSync, lstatSync, mkdtempSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { captureCandidate, candidateStillMatches } from "../src/candidate.ts"

function project(t: TestContext): { root: string; store: string } {
  const base = mkdtempSync(join(tmpdir(), "ironlint-candidate-test-"))
  t.after(() => rmSync(base, { recursive: true, force: true }))
  const root = join(base, "work")
  const store = join(base, "artifacts")
  mkdirSync(root)
  mkdirSync(store)
  execFileSync("git", ["init", "-q", root])
  return { root, store }
}

test("capture includes dirty, nonignored untracked, deletions, executable mode and ignores staged bytes", async (t) => {
  const { root, store } = project(t)
  writeFileSync(join(root, "staged.txt"), "stage")
  writeFileSync(join(root, "deleted.txt"), "remove")
  writeFileSync(join(root, "run.sh"), "echo pass\n")
  execFileSync("git", ["add", "-A"], { cwd: root })
  writeFileSync(join(root, "staged.txt"), "working")
  rmSync(join(root, "deleted.txt"))
  chmodSync(join(root, "run.sh"), 0o755)
  writeFileSync(join(root, "new.txt"), "untracked")
  const before = execFileSync("git", ["status", "--porcelain=v1"], { cwd: root }).toString()
  const candidate = await captureCandidate(root, store)
  assert.equal(readFileSync(join(candidate.tree, "staged.txt"), "utf8"), "working")
  assert.equal(readFileSync(join(candidate.tree, "new.txt"), "utf8"), "untracked")
  assert.equal(existsSync(join(candidate.tree, "deleted.txt")), false)
  assert.equal(lstatSync(join(candidate.tree, "run.sh")).mode & 0o111, 0o111)
  assert.equal(execFileSync("git", ["status", "--porcelain=v1"], { cwd: root }).toString(), before)
  assert.equal(await candidateStillMatches(candidate), true)
})

test("declared ignored inputs join identity and changing either tree invalidates capture", async (t) => {
  const { root, store } = project(t)
  writeFileSync(join(root, ".gitignore"), "secret.txt\n")
  writeFileSync(join(root, "secret.txt"), "one")
  const candidate = await captureCandidate(root, store, ["secret.txt"])
  assert.equal(readFileSync(join(candidate.tree, "secret.txt"), "utf8"), "one")
  writeFileSync(join(root, "secret.txt"), "two")
  assert.equal(await candidateStillMatches(candidate), false)
  writeFileSync(join(root, "secret.txt"), "one")
  writeFileSync(join(candidate.tree, "secret.txt"), "tampered")
  assert.equal(await candidateStillMatches(candidate), false)
})

test("capture rejects missing ignored inputs, symlinks, gitlinks, and internal artifact stores", async (t) => {
  const { root, store } = project(t)
  await assert.rejects(captureCandidate(root, store, ["missing.txt"]), /missing|unsupported/i)
  symlinkSync("missing", join(root, "link"))
  await assert.rejects(captureCandidate(root, store), /symlink|unsupported/i)
  rmSync(join(root, "link"))
  execFileSync("git", ["update-index", "--add", "--cacheinfo", "160000,1111111111111111111111111111111111111111,vendor"], { cwd: root })
  await assert.rejects(captureCandidate(root, store), /unsupported staged candidate entry/i)
  execFileSync("git", ["update-index", "--force-remove", "vendor"], { cwd: root })
  mkdirSync(join(root, "artifacts"))
  await assert.rejects(captureCandidate(root, join(root, "artifacts")), /outside/i)
  mkdirSync(join(root, "nested"))
  await assert.rejects(captureCandidate(join(root, "nested"), store), /top level/i)
})
