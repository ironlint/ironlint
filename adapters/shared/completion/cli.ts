import { runControlled } from "./completion.ts"
import { ClaudeHost } from "./claude.ts"
import { CodexHost } from "./codex.ts"

const required = ["root", "policy", "binary", "artifacts", "checks", "task", "runtime", "runtime-config"]
const optional = ["ignored-input", "max-repair-turns", "deadline-seconds"]

function parse(args: string[]): { harness: string; values: Map<string, string>; ignored: string[] } {
  const harness = args.shift()
  if (harness !== "codex" && harness !== "claude-code") throw new Error("select codex or claude-code completion")
  const values = new Map<string, string>()
  const ignored: string[] = []
  while (args.length) {
    const flag = args.shift()!
    const name = flag.startsWith("--") ? flag.slice(2) : ""
    if (![...required, ...optional].includes(name)) throw new Error("unknown completion argument")
    const value = args.shift()
    if (!value || value.startsWith("--")) throw new Error(`missing value for --${name}`)
    if (name === "ignored-input") ignored.push(value)
    else {
      if (values.has(name)) throw new Error(`duplicate --${name}`)
      values.set(name, value)
    }
  }
  for (const name of required) if (!values.has(name)) throw new Error(`missing --${name}`)
  return { harness, values, ignored }
}

function integer(value: string | undefined, fallback: number, minimum: number, maximum: number): number {
  if (value === undefined) return fallback
  if (!/^\d+$/.test(value)) throw new Error("completion budgets must be finite integers")
  const parsed = Number(value)
  if (!Number.isSafeInteger(parsed) || parsed < minimum || parsed > maximum) throw new Error("completion budget is out of range")
  return parsed
}

const controller = new AbortController()
const cancel = () => controller.abort()
process.on("SIGINT", cancel)
process.on("SIGTERM", cancel)
try {
  const { harness, values, ignored } = parse(process.argv.slice(2))
  const get = (name: string) => values.get(name)!
  const host = harness === "codex" ? new CodexHost(get("runtime"), get("runtime-config"))
    : new ClaudeHost(get("runtime"), get("runtime-config"))
  const result = await runControlled({ root: get("root"), policy: get("policy"), binary: get("binary"),
    artifacts: get("artifacts"), expectedIds: get("checks").split(","), prompt: get("task"), host,
    ignoredInputs: ignored, maxRepairTurns: integer(values.get("max-repair-turns"), 3, 0, 100),
    deadlineMs: integer(values.get("deadline-seconds"), 900, 1, 86_400) * 1000, signal: controller.signal })
  const terminal = { ...result, candidate: result.candidate && {
    identity: result.candidate.identity, tree: result.candidate.tree, artifact: result.candidate.artifact,
  } }
  process.stdout.write(JSON.stringify(terminal) + "\n")
  process.exitCode = result.status === "complete" ? 0 : 3
} catch (error) {
  process.stdout.write(JSON.stringify({ status: "incomplete", diagnostics: (error as Error).message, repairTurns: 0 }) + "\n")
  process.exitCode = 3
} finally {
  process.off("SIGINT", cancel)
  process.off("SIGTERM", cancel)
}
