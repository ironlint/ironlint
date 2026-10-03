import { performance } from "node:perf_hooks"

/** One attempt clock. Work stops early enough to include owned cleanup. */
export class Deadline {
  readonly signal: AbortSignal
  private readonly controller = new AbortController()
  private readonly end: number
  private readonly workEnd: number
  private readonly timer: ReturnType<typeof setTimeout>
  private readonly caller?: AbortSignal
  private readonly onCancel = () => this.controller.abort(new Error("completion cancelled"))
  private readonly now: () => number

  constructor(budgetMs = 900_000, caller?: AbortSignal, now: () => number = () => performance.now()) {
    if (!Number.isSafeInteger(budgetMs) || budgetMs <= 0) throw new Error("deadline budget must be a positive finite integer")
    this.now = now
    this.caller = caller
    this.end = now() + budgetMs
    this.workEnd = this.end - Math.min(2_000, budgetMs)
    this.signal = this.controller.signal
    this.timer = setTimeout(() => this.controller.abort(new Error("completion deadline exceeded")), Math.max(0, this.workEnd - now()))
    caller?.addEventListener("abort", this.onCancel, { once: true })
    if (caller?.aborted) this.onCancel()
  }

  remainingMs(cleanup = false): number {
    return Math.max(0, (cleanup ? this.end : this.workEnd) - this.now())
  }

  check(): void {
    if (!this.signal.aborted && this.remainingMs() === 0) this.controller.abort(new Error("completion deadline exceeded"))
    this.signal.throwIfAborted()
  }

  dispose(): void {
    clearTimeout(this.timer)
    this.caller?.removeEventListener("abort", this.onCancel)
  }
}
