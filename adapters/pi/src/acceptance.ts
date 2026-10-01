export type Acceptance =
  | { kind: "pass" }
  | { kind: "violation"; diagnostics: string }
  | { kind: "incomplete"; diagnostics: string }

type RecordValue = Record<string, unknown>

function object(value: unknown): value is RecordValue {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}

function bytes(value: unknown): value is number[] {
  return Array.isArray(value) && value.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)
}

function validResult(value: unknown): value is RecordValue {
  if (!object(value) || typeof value.id !== "string" || !value.id) return false
  if (!bytes(value.stdout) || !bytes(value.stderr)) return false
  if (value.stdout_truncated !== false || value.stderr_truncated !== false) return false
  if (value.reason !== null && typeof value.reason !== "string") return false
  if (value.outcome === "pass") return value.exit_status === 0
  return value.outcome === "violation" && Number.isInteger(value.exit_status)
    && (value.exit_status as number) >= 1 && (value.exit_status as number) <= 125
}

function output(value: unknown): string {
  if (!bytes(value)) return ""
  try { return new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(value)).trim() }
  catch { return "[invalid UTF-8 check output]" }
}

function resultDiagnostic(result: RecordValue): string {
  const parts = [result.reason, output(result.stdout), output(result.stderr)]
    .filter((part): part is string => typeof part === "string" && part.trim() !== "")
  const detail = parts.join("\n") || "check blocked"
  return `${result.id}: ${detail.slice(0, 1_500)}${detail.length > 1_500 ? " [diagnostic truncated]" : ""}`
}

function violationDiagnostics(results: RecordValue[]): string {
  const messages: string[] = []
  let total = 0
  for (const [index, result] of results.entries()) {
    const message = resultDiagnostic(result)
    if (total + message.length > 8_000) {
      messages.push(`[${results.length - index} more violating checks; reproduce acceptance for full output]`)
      break
    }
    messages.push(message)
    total += message.length + 1
  }
  return messages.join("\n")
}

export function classifyAcceptance(stdout: Buffer, exitCode: number, expectedIds: readonly string[]): Acceptance {
  const incomplete = (diagnostics: string): Acceptance => ({ kind: "incomplete", diagnostics })
  if (expectedIds.length === 0 || new Set(expectedIds).size !== expectedIds.length || expectedIds.some((id) => !id)) {
    return incomplete("invalid required check IDs")
  }
  let verdict: unknown
  try { verdict = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(stdout)) }
  catch { return incomplete("malformed or truncated acceptance result") }
  if (!object(verdict) || verdict.schema !== 7 || verdict.event !== "accept"
    || verdict.error !== null || !Array.isArray(verdict.not_run) || verdict.not_run.length !== 0
    || !Array.isArray(verdict.results) || !verdict.results.every(validResult)) {
    return incomplete("incomplete acceptance result")
  }
  const actual = verdict.results.map((result) => result.id as string)
  if (actual.length !== expectedIds.length || new Set(actual).size !== actual.length
    || actual.slice().sort().join("\0") !== expectedIds.slice().sort().join("\0")) {
    return incomplete("acceptance check set differs from the required check IDs")
  }
  if (exitCode === 0 && verdict.status === "pass" && verdict.results.every((result) => result.outcome === "pass")) {
    return { kind: "pass" }
  }
  if (exitCode === 2 && verdict.status === "violation" && verdict.results.some((result) => result.outcome === "violation")) {
    return { kind: "violation", diagnostics: violationDiagnostics(verdict.results.filter((result) => result.outcome === "violation")) }
  }
  return incomplete("acceptance did not finish with a complete pass or repairable violation")
}
