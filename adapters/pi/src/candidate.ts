import { execFile } from "node:child_process"
import { createHash } from "node:crypto"
import { promises as fs, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { isAbsolute, join, relative, resolve, sep } from "node:path"
import { promisify } from "node:util"

const exec = promisify(execFile)

type Entry = { path: string; mode: "100644" | "100755"; digest: string }

export interface Candidate {
  source: string
  artifact: string
  tree: string
  identity: string
  ignoredInputs: readonly string[]
  manifest: readonly Entry[]
  dispose(): void
}

async function git(root: string, args: string[], signal?: AbortSignal): Promise<Buffer> {
  const { stdout } = await exec("git", ["-c", "core.fsmonitor=false", "-C", root, ...args], {
    encoding: "buffer", maxBuffer: 64 * 1024 * 1024, signal,
  })
  return stdout as Buffer
}

function paths(bytes: Buffer): string[] {
  if (bytes.length === 0) return []
  const decoder = new TextDecoder("utf-8", { fatal: true })
  return bytes.subarray(0, bytes.length - Number(bytes.at(-1) === 0))
    .toString("binary").split("\0").map((part) => decoder.decode(Buffer.from(part, "binary")))
}

function safePath(path: string): void {
  if (!path || isAbsolute(path) || path.includes("\0") || path.split(/[\\/]/).some((part) => part === ".." || part === ".git")) {
    throw new Error(`unsupported candidate path: ${path}`)
  }
}

async function sourcePaths(root: string, ignoredInputs: readonly string[], signal?: AbortSignal): Promise<string[]> {
  const staged = paths(await git(root, ["ls-files", "--stage", "-z", "--full-name"], signal))
  for (const row of staged) {
    const tab = row.indexOf("\t")
    const mode = row.slice(0, 6)
    if (tab < 0 || (mode !== "100644" && mode !== "100755")) {
      throw new Error(`unsupported staged candidate entry: ${row.slice(tab + 1)}`)
    }
  }
  const listed = paths(await git(root, ["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--full-name"], signal))
  const all = new Set([...listed, ...ignoredInputs])
  for (const path of all) safePath(path)
  return [...all].sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))
}

async function assertRegular(root: string, path: string, signal?: AbortSignal): Promise<{ mode: Entry["mode"]; bytes: Buffer }> {
  let current = root
  const parts = path.split("/")
  for (const [index, part] of parts.entries()) {
    current = join(current, part)
    const info = await fs.lstat(current)
    if (info.isSymbolicLink()) throw new Error(`unsupported symlink: ${path}`)
    if (index !== parts.length - 1 && !info.isDirectory()) throw new Error(`unsupported candidate path: ${path}`)
  }
  const info = await fs.lstat(current)
  if (!info.isFile()) throw new Error(`unsupported candidate entry: ${path}`)
  return { mode: info.mode & 0o111 ? "100755" : "100644", bytes: await fs.readFile(current, { signal }) }
}

async function scanSource(root: string, ignoredInputs: readonly string[], destination?: string, signal?: AbortSignal): Promise<Entry[]> {
  const entries: Entry[] = []
  for (const path of await sourcePaths(root, ignoredInputs, signal)) {
    signal?.throwIfAborted()
    let file: Awaited<ReturnType<typeof assertRegular>>
    try {
      file = await assertRegular(root, path, signal)
    } catch (error) {
      if (!ignoredInputs.includes(path) && (error as NodeJS.ErrnoException).code === "ENOENT") continue
      throw error
    }
    if (destination) {
      const target = join(destination, path)
      await fs.mkdir(resolve(target, ".."), { recursive: true })
      await fs.writeFile(target, file.bytes, { mode: file.mode === "100755" ? 0o755 : 0o644, signal })
      await fs.chmod(target, file.mode === "100755" ? 0o755 : 0o644)
    }
    entries.push({ path, mode: file.mode, digest: createHash("sha256").update(file.bytes).digest("hex") })
  }
  return entries
}

async function scanTree(tree: string, directory = "", signal?: AbortSignal): Promise<Entry[]> {
  const entries: Entry[] = []
  for (const item of await fs.readdir(join(tree, directory), { withFileTypes: true })) {
    signal?.throwIfAborted()
    const path = directory ? `${directory}/${item.name}` : item.name
    if (item.isDirectory()) entries.push(...await scanTree(tree, path, signal))
    else {
      const file = await assertRegular(tree, path, signal)
      entries.push({ path, mode: file.mode, digest: createHash("sha256").update(file.bytes).digest("hex") })
    }
  }
  return entries.sort((a, b) => Buffer.compare(Buffer.from(a.path), Buffer.from(b.path)))
}

function identity(entries: readonly Entry[]): string {
  return createHash("sha256").update(JSON.stringify(entries)).digest("hex")
}

export async function captureCandidate(root: string, store = tmpdir(), ignoredInputs: readonly string[] = [], signal?: AbortSignal): Promise<Candidate> {
  signal?.throwIfAborted()
  const source = await fs.realpath(root)
  const top = await fs.realpath((await git(source, ["rev-parse", "--show-toplevel"], signal)).toString("utf8").trim())
  if (top !== source) throw new Error("candidate root must be the Git repository top level")
  const output = await fs.realpath(store)
  const relation = relative(source, output)
  if (relation === "" || (!relation.startsWith(`..${sep}`) && relation !== ".." && !isAbsolute(relation))) {
    throw new Error("candidate artifact store must be outside the source root")
  }
  for (const path of ignoredInputs) safePath(path)
  const artifact = await fs.mkdtemp(join(output, "ironlint-candidate-"))
  const tree = join(artifact, "tree")
  await fs.mkdir(tree)
  try {
    const manifest = await scanSource(source, ignoredInputs, tree, signal)
    const captured = identity(manifest)
    if (identity(await scanSource(source, ignoredInputs, undefined, signal)) !== captured || identity(await scanTree(tree, "", signal)) !== captured) {
      throw new Error("candidate changed during capture")
    }
    return {
      source, artifact, tree, identity: captured, ignoredInputs: [...ignoredInputs], manifest,
      dispose() { rmSync(artifact, { recursive: true, force: true }) },
    }
  } catch (error) {
    await fs.rm(artifact, { recursive: true, force: true })
    throw error
  }
}

export async function candidateStillMatches(candidate: Candidate, signal?: AbortSignal): Promise<boolean> {
  return await sourceStillMatches(candidate, signal) && await artifactStillMatches(candidate, signal)
}

export async function sourceStillMatches(candidate: Candidate, signal?: AbortSignal): Promise<boolean> {
  return identity(await scanSource(candidate.source, candidate.ignoredInputs, undefined, signal)) === candidate.identity
}

export async function artifactStillMatches(candidate: Candidate, signal?: AbortSignal): Promise<boolean> {
  return identity(await scanTree(candidate.tree, "", signal)) === candidate.identity
}
