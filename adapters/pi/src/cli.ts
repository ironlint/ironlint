import { resolve } from "node:path"
import { runControlled, type CompletionOptions } from "./completion.ts"

function parse(argv: string[]): CompletionOptions {
  const values = new Map<string, string>()
  const ignoredInputs: string[] = []
  const accepted = new Set(["--root", "--policy", "--binary", "--artifacts", "--checks", "--task", "--ignored-input", "--max-repair-turns", "--deadline-seconds", "--agent-dir"])
  for (let i = 0; i < argv.length; i += 2) {
    const flag = argv[i]
    const value = argv[i + 1]
    if (!flag || !accepted.has(flag) || value === undefined) throw new Error(`invalid option ${flag ?? ""}`)
    if (flag === "--ignored-input") ignoredInputs.push(value)
    else if (values.has(flag)) throw new Error(`duplicate option ${flag}`)
    else values.set(flag, value)
  }
  for (const flag of ["--policy", "--binary", "--artifacts", "--checks", "--task"]) {
    if (!values.get(flag)) throw new Error(`missing ${flag}`)
  }
  const repairs = values.has("--max-repair-turns") ? Number(values.get("--max-repair-turns")) : undefined
  const seconds = values.has("--deadline-seconds") ? Number(values.get("--deadline-seconds")) : undefined
  return {
    root: resolve(values.get("--root") ?? process.cwd()),
    policy: resolve(values.get("--policy")!), binary: resolve(values.get("--binary")!),
    artifacts: resolve(values.get("--artifacts")!), expectedIds: values.get("--checks")!.split(","),
    prompt: values.get("--task")!, ignoredInputs,
    ...(repairs === undefined ? {} : { maxRepairTurns: repairs }),
    ...(seconds === undefined ? {} : { deadlineMs: seconds * 1_000 }),
    ...(values.has("--agent-dir") ? { agentDir: resolve(values.get("--agent-dir")!) } : {}),
  }
}

const controller = new AbortController()
process.once("SIGINT", () => controller.abort())
process.once("SIGTERM", () => controller.abort())
let result: Awaited<ReturnType<typeof runControlled>>
try {
  const options = parse(process.argv.slice(2))
  result = await runControlled({ ...options, signal: controller.signal })
} catch (error) {
  result = { status: "incomplete", diagnostics: (error as Error).message, repairTurns: 0 }
}
const output = {
  status: result.status, diagnostics: result.diagnostics, repairTurns: result.repairTurns,
  ...(result.modelText === undefined ? {} : { modelText: result.modelText }),
  ...(result.candidate ? { candidate: { identity: result.candidate.identity, tree: result.candidate.tree,
    artifact: result.candidate.artifact } } : {}),
}
process.stdout.write(`${JSON.stringify(output)}\n`)
process.exitCode = result.status === "complete" ? 0 : 3
