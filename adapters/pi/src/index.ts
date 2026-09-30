// pi adapter for IronLint v1 completed-edit feedback.
import { spawn } from "node:child_process"
import { existsSync } from "node:fs"
import { join } from "node:path"

export type PiToolInput = {
  path?: string
  file_path?: string
  oldPath?: string
  newPath?: string
  old_path?: string
  new_path?: string
  from?: string
  to?: string
  paths?: string[]
}

type PiContent = { type: string; text?: string; [key: string]: unknown }
type ExecResult = { exitCode: number; stdout: string; stderr: string; cancelled: boolean }
type OwnedRun = { result: Promise<ExecResult>; cancel(): void }

interface ToolResultEvent {
  toolName?: string
  input?: PiToolInput
  content?: PiContent[]
  isError?: boolean
}

/** Minimal structural view of the pi extension API the adapter relies on. */
export interface PiExtensionAPI {
  on(event: string, handler: (event: never, ctx?: never) => unknown): void
  cwd?: string
  directory?: string
}

function resolveRoot(pi: PiExtensionAPI): string {
  return pi.cwd ?? pi.directory ?? process.cwd()
}

function getPath(input: PiToolInput): string | undefined {
  return input.path ?? input.file_path
}

/**
 * Returns known paths for a completed mutation. `undefined` means the tool
 * changed something but did not report paths, so the v1 evaluator receives an
 * unknown change set (no --file arguments).
 */
export function changedPaths(toolName: string, input: PiToolInput): string[] | undefined {
  switch (toolName) {
    case "write":
    case "edit":
    case "delete":
    case "unlink": {
      const path = getPath(input)
      return path ? [path] : undefined
    }
    case "rename":
    case "move": {
      const paths = [input.oldPath ?? input.old_path ?? input.from, input.newPath ?? input.new_path ?? input.to]
        .filter((path): path is string => typeof path === "string" && path.length > 0)
      return paths.length === 2 ? paths : undefined
    }
    case "batch":
      return Array.isArray(input.paths) && input.paths.every((path) => typeof path === "string")
        ? input.paths
        : undefined
    default:
      return undefined
  }
}

function trackedMutation(toolName: string): boolean {
  return new Set(["write", "edit", "delete", "unlink", "rename", "move", "batch"]).has(toolName)
}

const WALL_MS = 600_000
const STDOUT_BYTES = 8 * 1024 * 1024
const STDERR_BYTES = 64 * 1024
const CLOSE_GRACE_MS = 2_000

class ByteCapture {
  private readonly limit: number
  private bytes: Buffer | undefined
  private length = 0
  truncated = false

  constructor(limit: number) { this.limit = limit }

  append(chunk: Buffer): boolean {
    const count = Math.min(chunk.length, this.limit - this.length)
    if (count > 0) {
      this.bytes ??= Buffer.allocUnsafe(this.limit)
      chunk.copy(this.bytes, this.length, 0, count)
      this.length += count
    }
    this.truncated ||= count < chunk.length
    return !this.truncated
  }

  text(strict = false): string {
    if (strict && this.bytes) return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(this.bytes.subarray(0, this.length))
    return this.bytes?.toString("utf8", 0, this.length) ?? ""
  }

  clear(): void {
    this.bytes = undefined
    this.length = 0
  }
}

// A constant sink retains no invocation state. Late child/pipe errors remain
// harmless after the invocation's callbacks and capture buffers are removed.
const ignoreLateError = () => {}

class FeedbackRun implements OwnedRun {
  readonly result: Promise<ExecResult>
  private readonly child: ReturnType<typeof spawn>
  private readonly stdout = new ByteCapture(STDOUT_BYTES)
  private readonly stderr = new ByteCapture(STDERR_BYTES)
  private state: "active" | "stopping" | "settled" = "active"
  private cancelled = false
  private reason = ""
  private cleanupError = ""
  private wall: ReturnType<typeof setTimeout> | undefined
  private grace: ReturnType<typeof setTimeout> | undefined
  private resolve!: (result: ExecResult) => void

  constructor(args: string[], root: string) {
    this.child = spawn("ironlint", [...args, "--cancel-on-stdin-close"], {
      cwd: root,
      stdio: ["pipe", "pipe", "pipe"],
      detached: process.platform !== "win32",
    })
    this.result = new Promise((resolve) => { this.resolve = resolve })
    this.child.stdin?.on("error", this.onError)
    this.child.stdout!.on("data", this.onStdout).on("error", this.onError)
    this.child.stderr!.on("data", this.onStderr).on("error", this.onError)
    this.child.on("error", this.onError).on("close", this.onClose)
    this.wall = setTimeout(() => this.stop("incomplete feedback: 600-second wall-time limit exceeded"), WALL_MS)
  }

  cancel(): void {
    if (this.state === "settled") return
    this.cancelled = true
    this.stdout.clear()
    this.stderr.clear()
    this.stop("feedback cancelled by a newer mutation")
  }

  private readonly onStdout = (chunk: Buffer) => {
    if (this.state === "active" && !this.stdout.append(chunk)) {
      this.stop("incomplete feedback: 8 MiB stdout byte limit exceeded")
    }
  }

  private readonly onStderr = (chunk: Buffer) => {
    if (this.state === "active") this.stderr.append(chunk)
  }

  private readonly onError = (error: Error) => {
    if (this.state === "stopping") this.cleanupError ||= `child cleanup failed: ${error.message}`
    else this.stop(`incomplete feedback: ${error.message}`)
  }

  private readonly onClose = (code: number | null) => { this.finish(code, true) }

  private stop(reason: string): void {
    if (this.state !== "active") return
    this.state = "stopping"
    this.reason = reason
    this.stdout.clear()
    clearTimeout(this.wall)
    this.grace = setTimeout(() => {
      this.killOwnedChild()
      this.finish(null, false)
    }, CLOSE_GRACE_MS)
    this.child.stdin?.end()
  }

  private killOwnedChild(): void {
    if (process.platform !== "win32" && this.child.pid) {
      try {
        process.kill(-this.child.pid, "SIGKILL")
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ESRCH") {
          this.cleanupError = `process-group cleanup failed: ${(error as Error).message}`
        }
      }
    }
    try {
      this.child.kill("SIGKILL")
    } catch (error) {
      this.cleanupError = `direct-child cleanup failed: ${(error as Error).message}`
    }
  }

  private finish(code: number | null, closed: boolean): void {
    if (this.state === "settled") return
    this.state = "settled"
    clearTimeout(this.wall)
    clearTimeout(this.grace)
    this.child.off("error", this.onError).off("close", this.onClose).on("error", ignoreLateError)
    this.child.stdin?.off("error", this.onError).on("error", ignoreLateError).destroy()
    for (const [stream, listener] of [[this.child.stdout, this.onStdout], [this.child.stderr, this.onStderr]] as const) {
      stream!.off("data", listener).off("error", this.onError).on("error", ignoreLateError)
      stream!.destroy()
    }
    if (!closed) {
      this.cleanupError ||= "direct-child closure was not observed within 2 seconds"
      this.child.unref()
    }
    let stdout = ""
    try { stdout = this.stdout.text(true) } catch {
      this.reason ||= "incomplete feedback: invalid UTF-8 stdout"
    }
    const diagnostics = [this.stderr.text()]
    if (this.stderr.truncated) diagnostics.push("stderr was truncated at 64 KiB")
    if (this.reason) diagnostics.push(this.reason)
    if (this.cleanupError) diagnostics.push(this.cleanupError)
    let exitCode = this.reason || this.cleanupError ? 3 : code ?? 3
    if (exitCode === 0 && !completeChangeFeedback(stdout)) {
      exitCode = 3
      stdout = ""
      diagnostics.push("incomplete feedback: invalid schema 7 change JSON")
    }
    if (this.cancelled && this.cleanupError) console.error(`IronLint: ${this.cleanupError}`)
    this.stdout.clear()
    this.stderr.clear()
    this.resolve({ exitCode, stdout, stderr: diagnostics.filter(Boolean).join("\n"), cancelled: this.cancelled })
  }
}

function startIronLint(args: string[], root: string): OwnedRun {
  try {
    return new FeedbackRun(args, root)
  } catch (error) {
    return {
      result: Promise.resolve({ exitCode: 3, stdout: "", stderr: `incomplete feedback: ${(error as Error).message}`, cancelled: false }),
      cancel() {},
    }
  }
}

export function runIronLint(args: string[], root: string): Promise<ExecResult> {
  return startIronLint(args, root).result
}

function byteArray(value: unknown): boolean {
  return Array.isArray(value) && value.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)
}

function completePassResult(value: unknown): boolean {
  if (!value || typeof value !== "object") return false
  const result = value as Record<string, unknown>
  return typeof result.id === "string" && result.id.length > 0
    && result.outcome === "pass" && result.exit_status === 0 && result.reason === null
    && byteArray(result.stdout) && byteArray(result.stderr)
    && typeof result.stdout_truncated === "boolean" && typeof result.stderr_truncated === "boolean"
}

function completeChangeFeedback(stdout: string): boolean {
  try {
    const verdict = JSON.parse(stdout) as Record<string, unknown> | null
    if (!verdict || verdict.schema !== 7 || verdict.event !== "change" || verdict.error !== null) return false
    if (!Array.isArray(verdict.not_run) || verdict.not_run.length !== 0 || !Array.isArray(verdict.results)) return false
    if (verdict.status === "not_run") return verdict.results.length === 0
    return verdict.status === "pass" && verdict.results.length > 0 && verdict.results.every(completePassResult)
  } catch {
    return false
  }
}

type VerdictResult = {
  id?: unknown
  outcome?: unknown
  reason?: unknown
  stdout?: unknown
  stderr?: unknown
}

function diagnostic(value: unknown): string {
  if (typeof value === "string" && value.trim()) return value.trim()
  if (Array.isArray(value) && value.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)) {
    return Buffer.from(value).toString("utf8").trim()
  }
  return ""
}

export function feedback(stdout: string, fallback: string): string {
  try {
    const verdict = JSON.parse(stdout) as { results?: VerdictResult[]; error?: unknown }
    const results = (verdict.results ?? []).filter(
      (result) => result.outcome === "violation" || result.outcome === "error",
    )
    const lines = results.map((result) => {
      const id = typeof result.id === "string" ? result.id : "check"
      return `${id}: ${diagnostic(result.reason) || diagnostic(result.stderr) || diagnostic(result.stdout) || fallback}`
    })
    if (lines.length > 0) return lines.join("\n")
    if (typeof verdict.error === "string" && verdict.error.trim()) return verdict.error.trim()
  } catch {
    // The reproduction command below remains useful when a future CLI changes JSON.
  }
  return fallback
}

function reproduction(args: string[]): string {
  return `ironlint ${args.map((arg) => (/^[A-Za-z0-9_./:-]+$/.test(arg) ? arg : `'${arg.replaceAll("'", "'\\''")}'`)).join(" ")}`
}

function feedbackText(result: ExecResult, command: string, superseded: boolean): string {
  const fallback = result.stderr.trim() || result.stdout.trim() || `ironlint exited ${result.exitCode}`
  const prefix = result.exitCode === 2 ? "IronLint change feedback" : "IronLint could not evaluate this change"
  const suffix = superseded ? "\nThis result was superseded by a newer edit." : ""
  return `${prefix}: ${feedback(result.stdout, fallback)}\nReproduce: ${command}${suffix}`
}

export default function ironlintExtension(pi: PiExtensionAPI): void {
  const projectRoot = resolveRoot(pi)
  const configPath = join(projectRoot, ".ironlint.yml")
  let newestEdit = 0
  let active: OwnedRun | undefined

  pi.on("tool_result", async (event: ToolResultEvent) => {
    const toolName = event?.toolName
    if (!toolName || !trackedMutation(toolName) || event.isError) return

    const paths = changedPaths(toolName, event.input ?? {})
    if (paths?.length === 0) return

    const edit = ++newestEdit
    active?.cancel()
    active = undefined
    if (!existsSync(configPath)) return

    const args = ["check", "--event", "change", "--config", configPath, "--root", projectRoot, "--format", "json"]
    if (paths) {
      for (const path of paths) args.push("--file", path)
    }
    const run = startIronLint(args, projectRoot)
    active = run
    const result = await run.result
    if (active === run) active = undefined
    if (result.cancelled || result.exitCode === 0) return
    return {
      content: [...(event.content ?? []), { type: "text", text: feedbackText(result, reproduction(args), edit !== newestEdit) }],
    }
  })
}
