Act as a skeptical systems architect. Assess whether IronLint’s core idea is worth keeping, and propose the smallest viable redesign. Do not preserve existing architecture by default.

Goal: give AI coding agents deterministic, early feedback that proposed changes violate developer-declared static rules. Rules live in `.ironlint.yml`, run arbitrary CI-style commands, and target file globs or lifecycle events. Core must be harness-agnostic; harness integrations are adapters.

Current tension:
- Pre-write gating can pass proposed content on stdin, so rejected content never reaches disk.
- But blocking normal writes breaks productive workflows, especially TDD and incremental edits.
- Some tools can only validate after writing.
- Git pre-commit is late and agents can bypass it with `--no-verify`.
- The current design adds a Bash gate to prevent bypasses or policy/hook tampering, but this grows into a brittle command-classification security boundary.
- “Writes never land on disk” is increasingly not a credible universal promise across harnesses and tools.

Current adapter pattern:

```ts
const proposed = computeProposedContent(toolName, filePath, input)
if (proposed === null) return // cannot faithfully simulate: skip gate

const res = runIronLint(
  ["check", "--file", filePath, "--content", "-", "--config", configPath, "--format", "json"],
  proposed,
)
if (res.exitCode === 2) return { block: true, reason: blockReason(res.stdout) }
if (res.exitCode === 3) return failOpenOrClosed("check", res.stderr.trim())
```

Current self-defense pattern:

```ts
if (toolName === "bash") {
  const res = runIronLint(["gate-bash"], command)
  if (res.exitCode === 0) return
  if (res.exitCode === 2) return { block: true, reason: res.stdout }
  return { block: true, reason: "bash-gate failed — fail-closed" }
}
```

The Bash gate currently blocks `ironlint trust`, edits to policy/adapter/hook surfaces, Git hook bypasses such as `--no-verify`, and destructive shell commands against protected paths. It cannot reliably solve shell indirection.

Evaluate from first principles:

1. What problem should this tool actually promise to solve?
2. Which guarantees are impossible or counterproductive across heterogeneous agent harnesses?
3. Should pre-write blocking, post-write feedback, commit enforcement, and policy tamper protection be separate modes/products rather than one system?
4. What is the minimum core abstraction and configuration model that remains valuable?
5. Where should responsibility end: core, adapter, CI, repository permissions, or the harness?
6. Give 2–3 concrete product directions, rank them by usefulness vs. complexity, and recommend one.
7. Include a migration/teardown plan: what to delete, freeze, or retain from the existing design.

Optimize for developer usability, honest guarantees, deterministic behavior, and the smallest implementation surface. Be direct; call out if the project should be radically simplified or abandoned.
