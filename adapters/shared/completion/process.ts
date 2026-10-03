import { spawn } from "node:child_process"
import type { Deadline } from "./deadline.ts"

export interface OwnedOptions {
  executable: string
  args: readonly string[]
  root: string
  deadline: Deadline
  env?: NodeJS.ProcessEnv
  input?: string
  maxMs?: number
}

export interface OwnedResult { code: number; stdout: Buffer; stderr: Buffer }

export class CleanupUnconfirmedError extends Error {
  readonly settled: Promise<void>
  constructor(message: string, settled: Promise<void>) { super(message); this.settled = settled }
}

/** POSIX process owner: no output escapes until the child and pipes settle. */
export async function runOwned(options: OwnedOptions): Promise<OwnedResult> {
  options.deadline.check()
  if (process.platform === "win32") throw new Error("controlled completion requires a POSIX process owner")
  return new Promise<OwnedResult>((resolve, reject) => {
    const child = spawn(options.executable, [...options.args], {
      cwd: options.root, env: options.env ?? process.env, detached: true, stdio: ["pipe", "pipe", "pipe"],
    })
    let stdout = Buffer.alloc(0)
    let stderr = Buffer.alloc(0)
    let reason = ""
    let done = false
    let cleaning = false
    let leaderClosed = false
    let closedCode: number | null = null
    let grace: ReturnType<typeof setTimeout> | undefined
    let hardCutoff: ReturnType<typeof setTimeout> | undefined
    let groupPoll: ReturnType<typeof setTimeout> | undefined
    let confirmSettlement!: () => void
    const settled = new Promise<void>((resolve) => { confirmSettlement = resolve })
    const limit = setTimeout(() => stop("owned process exceeded its time limit"), Math.min(options.maxMs ?? 600_000, options.deadline.remainingMs()))
    const kill = (signal: NodeJS.Signals): boolean => {
      if (!child.pid) return false
      try { process.kill(-child.pid, signal); return true }
      catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "ESRCH") reason ||= `owned process cleanup failed: ${(error as Error).message}`
        return false
      }
    }
    const groupExists = (): boolean => {
      if (!child.pid) return false
      try { process.kill(-child.pid, 0); return true }
      catch (error) { return (error as NodeJS.ErrnoException).code !== "ESRCH" }
    }
    const finish = (code: number | null, confirmed = true) => {
      if (done) return
      done = true
      clearTimeout(limit); clearTimeout(grace); clearTimeout(hardCutoff)
      options.deadline.signal.removeEventListener("abort", onAbort)
      if (confirmed) clearTimeout(groupPoll)
      child.stdin?.destroy(); child.stdout?.destroy(); child.stderr?.destroy()
      if (!confirmed) { child.unref(); groupPoll?.unref() }
      const diagnostics = reason + (stderr.length ? `\n${stderr.toString("utf8").slice(0, 2_000)}` : "")
      if (!confirmed) reject(new CleanupUnconfirmedError(`${diagnostics}\nowned process cleanup unconfirmed; workspace must not be reused until settlement`, settled))
      else if (reason) reject(new Error(diagnostics))
      else if (code === null) reject(new Error("owned process did not settle normally"))
      else resolve({ code, stdout, stderr })
    }
    function stop(message: string, background = false): void {
      if (done || cleaning) return
      cleaning = true
      reason ||= message
      stdout = Buffer.alloc(0)
      child.stdin?.end()
      kill(background ? "SIGKILL" : "SIGTERM")
      const cleanupMs = Math.min(2_000, options.deadline.remainingMs(true))
      grace = setTimeout(() => {
        kill("SIGKILL")
      }, Math.min(1_000, cleanupMs / 2))
      hardCutoff = setTimeout(() => {
        kill("SIGKILL")
        // A timer cannot confirm settlement. Withhold success and keep owned
        // artifacts/locks until the eventual close event instead of reusing them.
        finish(null, false)
      }, cleanupMs)
    }
    function observeSettlement(): void {
      if (leaderClosed && !groupExists()) {
        clearTimeout(groupPoll)
        confirmSettlement()
        finish(closedCode)
        return
      }
      if (!groupPoll) {
        groupPoll = setTimeout(() => { groupPoll = undefined; observeSettlement() }, 20)
        // A poisoned attempt remains unavailable after the hard cutoff, but
        // eventual cleanup must not keep a cancelled CLI alive indefinitely.
        if (done) groupPoll.unref()
      }
    }
    const onAbort = () => stop((options.deadline.signal.reason as Error)?.message ?? "completion cancelled")
    child.stdout!.on("data", (chunk: Buffer) => {
      if (reason) return
      if (stdout.length + chunk.length > 8 * 1024 * 1024) stop("owned process stdout exceeded the 8 MiB limit")
      else stdout = Buffer.concat([stdout, chunk])
    })
    child.stderr!.on("data", (chunk: Buffer) => {
      const keep = Math.min(chunk.length, Math.max(0, 64 * 1024 - stderr.length))
      if (keep) stderr = Buffer.concat([stderr, chunk.subarray(0, keep)])
    })
    child.stdin!.on("error", (error: NodeJS.ErrnoException) => { if (error.code !== "EPIPE") stop(`owned process input failed: ${error.message}`) })
    child.stdout!.on("error", (error) => stop(`owned process output failed: ${error.message}`))
    child.stderr!.on("error", (error) => stop(`owned process output failed: ${error.message}`))
    child.on("error", (error) => stop(`owned process failed: ${error.message}`))
    child.on("close", (code) => {
      leaderClosed = true
      closedCode = code
      // Pipe/leader close does not settle descendants which discarded their
      // pipes. Retain ownership until the entire process group disappears.
      if (groupExists()) stop("owned process left background work after exit", true)
      observeSettlement()
    })
    options.deadline.signal.addEventListener("abort", onAbort, { once: true })
    if (options.deadline.signal.aborted) onAbort()
    if (options.input !== undefined) child.stdin!.end(options.input)
  })
}
