# Plans

Implementation plans IronLint is built from. Each plan is a self-contained, multi-step, checkbox-tracked design + execution doc following the `superpowers:writing-plans` format. Plans are executed via `superpowers:executing-plans` (inline) or `superpowers:subagent-driven-development` (subagent-per-task).

A plan owns its own progress via its checkboxes — that's the source of truth. This README is a navigation surface; keep it in sync by hand when state changes meaningfully (new plan added, plan completed, priority shifted). The `Future` section below is the closest thing to a backlog: short bullets for work that hasn't graduated to a plan file yet.

**Layout:**

- `plans/*.md` — in-flight or queued
- `plans/archive/*.md` — completed (frozen design records, useful as "how was X built" context)

## Active

- [`2026-08-17-floor-hook-spec-completion`](2026-08-17-floor-hook-spec-completion.md) — pre-filter removal (adapters route all Bash to gate-bash), GIT_CONFIG env-injection gate, empty-fixture CI failure, spec closeout, trust + commit + smoke.

## Future

Ideas that haven't graduated to plans. When something here has enough definition to write a plan against, lift it into a dated plan file.

- **[2026-08-19]** Harness-watch CI job — weekly per-harness latest-version vs fixture provenance stamp plus docs-hash drift watch. _Why:_ auto-file drift issues feeding the `adapter-drift-audit` skill before a harness payload change ships silently.
- **[2026-08-19]** `ironlint gate-tool` — W5: `--harness <id>` dialect parse/response moves into the binary so the adapter shims collapse to stdin→binary→stdout wiring. _Why:_ deletes jq from the shell hooks and shrinks the pi/opencode TS shims.
- **[2026-08-19]** Live-capture contract tier — headless harness runs in CI regenerate pinned fixtures and open an auto-PR on drift. _Why:_ replaces the manual capture procedure deliberately deferred from W4.
- **D2 `ironlint coverage`** and **D3 `ironlint debt`** ([spec §D](../specs/2026-05-12-bully-parity-closures.md)) — telemetry-derived rule-coverage and tech-debt reports. D1 (typed telemetry) shipped; these consume it.
- **A4 `context.lines`** — per-rule context-line count override on the semantic prompt.
- **C5 `validate --execute-dry-run`** — invoke `script:` rules in a sandbox during `validate`, surface failures early.
- **F1 declarative session rules** — `when.changed_any` / `require.changed_any` as a deterministic alternative to LLM-driven session eval.
- ~~**G1 trust+rules split CI lint** (stop-gap; full trust-model decision blocks 0.3 freeze).~~ (resolved by the 2026-06-24 trust store)

### Shipped without a plan file

Small/medium changes that landed direct-to-`main` without a dedicated plan; recorded here so they aren't invisible.

- **2026-05-22** — E2 script-engine output modes (`Parsed` / `Passthrough`); see [`CHANGELOG.md`](../CHANGELOG.md#unreleased) and commit `3241026`.
- **2026-05-22** — OpenCode adapter pre-flight gate (moved to `tool.execute.before` + shadow-write + late-init fix); commit `069cc74`.
- **2026-05-22** — macOS capability-warning dedup (once per process instead of per script invocation); commit `f47ef82`.
- **2026-05-23** — H4 spec + docs walkback: `specs/overview.md` §7.1 rewritten as two-paths (direct-API + subagent), §11.5 marked resolved, H3 plan archived. See merge commit.

## Archive

Completed plans live in [`archive/`](archive/). They're frozen design records.

- [`2026-05-11-ironlint-0.1a-foundation`](archive/2026-05-11-ironlint-0.1a-foundation.md) — workspace skeleton, script engine, trust gate.
- [`2026-05-11-ironlint-0.1b-engines`](archive/2026-05-11-ironlint-0.1b-engines.md) — `ast`, `semantic`, `session` engines + `init` / `migrate` / `baseline` / `session record` commands.
- [`2026-05-11-ironlint-0.1c-claude-code-adapter`](archive/2026-05-11-ironlint-0.1c-claude-code-adapter.md) — Claude Code adapter (`plugin.json`, PostToolUse + Stop hooks, ported skills).
- [`2026-05-12-ironlint-opencode-adapter`](archive/2026-05-12-ironlint-opencode-adapter.md) — OpenCode adapter at parity with the Claude Code adapter.
- [`2026-05-12-bug-audit-remediation`](archive/2026-05-12-bug-audit-remediation.md) — remediation campaign for the [P0/P1/P2 findings](../docs/audits/2026-05-12-bug-audit.md) from the 2026-05-12 audit.
- [`2026-05-12-ironlint-a1-prompt-injection`](archive/2026-05-12-ironlint-a1-prompt-injection.md) — `<TRUSTED_POLICY>` / `<UNTRUSTED_EVIDENCE>` sentinel boundary in semantic prompt.
- [`2026-05-12-ironlint-a2-skip-patterns`](archive/2026-05-12-ironlint-a2-skip-patterns.md) — built-in skip patterns + project `skip:` + `~/.ironlint-ignore`.
- [`2026-05-12-ironlint-a3-diff-prefilter`](archive/2026-05-12-ironlint-a3-diff-prefilter.md) — local `can_match_diff` short-circuit for `engine: semantic`; new `reason` field on telemetry; runner-side wiring.
- [`2026-05-12-ironlint-e1-baseline-checksum`](archive/2026-05-12-ironlint-e1-baseline-checksum.md) — `line_sha256` fingerprinting in `Baseline`; v1-format read tolerance; new `ironlint baseline refresh` subcommand.
- [`2026-05-12-ironlint-b1-parallel-rules`](archive/2026-05-12-ironlint-b1-parallel-rules.md) — rayon-driven parallel rule dispatch in `IronLintEngine::check`; `execution.max_workers` config + `IRONLINT_MAX_WORKERS` env override; deterministic output order via BTreeMap iteration.
- [`2026-05-12-ironlint-c4-check-flags`](archive/2026-05-12-ironlint-c4-check-flags.md) — `ironlint check --rule <id>` (repeatable) restricts evaluation upstream of the parallel pool; `--explain` prints a per-rule outcome report to stderr; `--print-prompt` renders the semantic prompt without dispatching to the LLM.
- [`2026-05-13-ironlint-d1-typed-telemetry`](archive/2026-05-13-ironlint-d1-typed-telemetry.md) — typed `LogEntry` enum (`session_init` / `check` / `semantic_verdict` / `semantic_skipped`) with `PerRuleRecord` nesting and a `read_all` legacy reader. Foundation for D2/D3.
- [`2026-05-13-ironlint-c1-doctor`](archive/2026-05-13-ironlint-c1-doctor.md) — `ironlint doctor` diagnostic subcommand: 9 checks (binary, config, parses, trust, schema, scope_globs, engines, adapter, runtime_state); JSON contract under `docs/doctor.md`; exit code 0 on pass-or-warn, 1 on any fail.
- [`2026-05-13-ironlint-c2-explain-guide`](archive/2026-05-13-ironlint-c2-explain-guide.md) — `ironlint explain <file>` and `ironlint guide <file>` read-only inspection subcommands; shared `scope_outcomes` helper in `ironlint-core`; JSON snapshots locked with insta.
- [`2026-05-13-ironlint-c3-show-resolved-config`](archive/2026-05-13-ironlint-c3-show-resolved-config.md) — `ironlint show-resolved-config` (TSV / YAML / JSON) with per-rule origin tracking; `extends::resolve_with_origin` core helper.
- [`2026-05-14-ironlint-h1-emit-semantic-payload`](archive/2026-05-14-ironlint-h1-emit-semantic-payload.md) — `ironlint check --emit-semantic-payload` flag + `llm.provider: claude-code-subagent` provider arm + `DeferredVerdict` envelope (`ironlint_core::verdict_deferred`); enables H3 (Claude Code adapter subagent mode).
- [`2026-05-14-ironlint-h2-record-verdict`](archive/2026-05-14-ironlint-h2-record-verdict.md) — `ironlint record-verdict` subcommand appends one `SemanticVerdict` record to `.ironlint/log.jsonl`; consumed by the Claude Code interpreter skill (H3) to keep coverage reports accurate under subagent-mediated semantic eval.
- [`2026-05-14-ironlint-h3-adapter-subagent-mode`](archive/2026-05-14-ironlint-h3-adapter-subagent-mode.md) — Claude Code adapter subagent mode: hook routes `llm.provider: claude-code-subagent` through `--emit-semantic-payload`, wraps the result in `hookSpecificOutput.additionalContext`; new `ironlint` interpreter skill + `ironlint-evaluator` subagent close the loop. Restores bully parity for subscription users.
- [`2026-06-24-ironlint-trust-store`](archive/2026-06-24-ironlint-trust-store.md) — out-of-repo trust store; check fails closed until blessed.

## Conventions

- One feature/refactor per plan file. Filename: `YYYY-MM-DD-<short-slug>.md`.
- Plan header carries Goal, Architecture, Tech Stack, and (when relevant) Severity + Sequencing. See `superpowers:writing-plans` for the canonical format.
- Tasks are bite-sized (2–5 min) and checkbox-tracked. The TDD cycle (write failing test → verify red → minimal impl → verify green → commit) is the per-task default per repo rules.
- Substantive work — multi-file, multi-phase, architectural — earns a plan. A 3-line "fix X" doesn't; just commit it.
- When a plan ships, `git mv` it to `archive/` and strike its row from the table above.
