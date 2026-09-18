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
type ExecResult = { exitCode: number; stdout: string; stderr: string }

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

export function runIronLint(args: string[], root: string): Promise<ExecResult> {
  return new Promise((resolve) => {
    const child = spawn("ironlint", args, { cwd: root, stdio: ["ignore", "pipe", "pipe"] })
    let stdout = ""
    let stderr = ""
    let settled = false
    const finish = (result: ExecResult) => {
      if (!settled) {
        settled = true
        resolve(result)
      }
    }
    child.stdout.setEncoding("utf8").on("data", (chunk: string) => {
      stdout += chunk
    })
    child.stderr.setEncoding("utf8").on("data", (chunk: string) => {
      stderr += chunk
    })
    child.on("error", (error: Error) => finish({ exitCode: -1, stdout, stderr: error.message }))
    child.on("close", (code: number | null) => finish({ exitCode: code ?? -1, stdout, stderr }))
  })
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

  pi.on("tool_result", async (event: ToolResultEvent) => {
    const toolName = event?.toolName
    if (!toolName || !trackedMutation(toolName) || event.isError) return

    const paths = changedPaths(toolName, event.input ?? {})
    if (paths?.length === 0) return

    const edit = ++newestEdit
    if (!existsSync(configPath)) return

    const args = ["check", "--event", "change", "--config", configPath, "--root", projectRoot, "--format", "json"]
    if (paths) {
      for (const path of paths) args.push("--file", path)
    }
    const result = await runIronLint(args, projectRoot)
    if (result.exitCode === 0) return
    return {
      content: [...(event.content ?? []), { type: "text", text: feedbackText(result, reproduction(args), edit !== newestEdit) }],
    }
  })
}
