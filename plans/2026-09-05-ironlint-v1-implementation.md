# IronLint v1.0 implementation — Terra coordinator, Luna workers

**Status:** In progress. The versioned core/CLI path is implemented; external acceptance integration, verified feedback, migration, teardown, and release evidence remain open.
**Source of truth:** [v1 specification](../specs/2026-09-05-ironlint-v1-design.md).
**Goal:** Ship deterministic, fail-closed acceptance with editable intermediate states, one verified feedback adapter, and safe removal of the old local self-defense system.
**Architecture:** Existing Rust evaluator and CLI, thin harness adapters, externally owned acceptance authority; no new service.
**Tech stack:** Existing Cargo workspace, existing adapter languages and test tooling, native repository acceptance controls.
**Sequencing:** Prove the authority boundary, freeze contracts, implement small slices, migrate installations, then delete legacy paths.

## 1. Execution instructions for Terra

You are the coordinator, using `gpt-5.6-terra`. Delegate bounded implementation and independent review tasks to `gpt-5.6-luna`. You own architecture, public contracts, authority decisions, integration, and the final release verdict. A worker reporting green is evidence to inspect, not permission to mark a phase complete.

Read the spec and repository instructions before dispatch. The spec describes a new version; current `AGENTS.md` invariants describe 0.4. Preserve those invariants on the transition path and implement new semantics only on the versioned v1 path. Do not silently resolve a contradiction by weakening either contract. Record any necessary spec clarification here before dependent work starts.

Use at most three Luna workers concurrently, leaving one slot for Terra. Run fewer when tasks share files or depend on an unfinished contract. Use explicit model overrides when spawning; provide a bounded context packet rather than the whole conversation. Workers must not spawn further agents. Do not create separate user-visible Codex tasks for these subtasks.

Terra must:

- Keep an ownership table of active task IDs and writable files in this plan; only Terra edits the plan.
- Split each packet into small failing-test → implementation → passing-test steps; each dispatch should own one behavior or tightly coupled behavior group, not a whole subsystem rewrite.
- Let a worker request a needed shared-file change; transfer ownership or make it yourself after the current owner stops.
- Own workspace manifests, dependency/lockfile changes, public module exports, version switches, and final deletion integration.
- Run only one Cargo build/test/coverage operation against a shared target directory at a time; parallel code work does not imply parallel workspace builds.
- Inspect a worker's diff and evidence, then request a separate agent review after each coding packet, as required by this repository.
- Resolve findings before releasing dependent packets. Use the adversarial-review skill for diff reviews.
- Stop a worker after repeated failed fixes or scope expansion, inspect the cause, and issue a smaller corrected packet; do not let Luna improvise architecture.
- Preserve unrelated user changes. At plan creation these included `crates/ironlint-bash-gate/src/lib.rs` and `astra-prompt.md`; resnapshot at execution time. Later deletion of that crate requires reconciling the user's changed file, not discarding it as obsolete.
- Make no commits, pushes, releases, or repository permission changes unless the execution request authorizes them. Preparing and testing code does not require publishing it.

Shared checkout is the default with exclusive file ownership. Do not introduce an orchestration framework, automatic cherry-pick pipeline, or per-worker worktrees unless a concrete collision makes isolation necessary.

### Copyable worker packet

```text
Task ID and goal:
Model: gpt-5.6-luna; do not delegate.
Spec sections and frozen contract/fixture paths:
Relevant graph nodes/source spans:
Allowed writable files (exclusive ownership):
Read-only dependencies:
Required behavior and explicit non-goals:
Small steps: failing test, confirm failure, minimal implementation, targeted check.
Test command and expected observable result:
Stop conditions: missing contract, required edit outside ownership, unclear authority.
Do not commit, change trust, edit installed hooks, or weaken tests.
Return: files changed; behavior; commands and actual results; unresolved issues.
```

### Copyable independent review packet

```text
Review task <ID> against spec sections <N> and its frozen fixtures.
Model: gpt-5.6-luna; read-only; do not delegate.
Read the adversarial-review skill. Review the actual diff and named callers.
For each finding give a concrete failing input/state, location, and required fix.
Check especially <packet-specific failure mechanisms>.
Do not treat passing tests or the author's report as proof of correctness.
Return actionable findings or no findings, and identify anything not verified.
```

## 2. Phase 0 — baseline and feasibility (Terra owns)

- [ ] Record branch, dirty files, installed-tool availability, and existing checks before changing anything; do not reset or clean the checkout.
- [ ] Use the codebase graph and Graft to map config parsing, selection, `engine::gate`, runner, verdict, trust, CLI, installation, and adapter consumers; verify current paths before assigning file ownership.
- [ ] Record baseline workspace tests, adapter tests, clippy, and coverage status; distinguish pre-existing failures from regressions.
- [ ] Inspect existing CI and repository integration conventions. Select one actual reference repository platform and one feedback harness based on accessible test evidence, not familiarity.
- [ ] Run `adapter-drift-audit` for the candidate harness and inspect its current post-operation event and diagnostic delivery contract. Existing Codex edit captures are a lead, not evidence of post-edit support.
- [ ] Resolve the reference integration's six authority obligations from spec §4 in a concrete deployment table: protected operation, all routes, revision binding, policy authority, evaluator provenance, and execution isolation.

**Feasibility deliverable:** Extend this plan with the selected platform/harness, pinned versions, exact protected operation, actual proof procedure, and evidence location. This is a prerequisite to implementation packets that depend on them. Consult current primary documentation through the repository's required documentation tools when implementing platform or harness configuration; do not invent hook names or CI syntax from this plan.

The reference integration must place result publication credentials outside the process environment, filesystem, and privilege domain accessible to candidate execution. A candidate-modifiable workflow or script that can publish success does not satisfy the contract. Prefer existing platform isolation and protected configuration. Do not build a broker or security service to close a platform gap.

Include in the proof: a failing candidate, a candidate attempting to replace the policy/evaluator/script, a forged success artifact, a new revision after a passing result, and the protected operation attempted through an alternative route. If merged state is accepted, test changed merge inputs. Define explicitly which scripts/tests are approved inputs and which are delegated to candidate content.

**Go/no-go:** A plausible diagram is insufficient. Obtain a runnable boundary proof using simple commands before investing in the v1 runner. Local mocks may prove the caller's logic but cannot establish repository enforcement. If credentials or a disposable test repository are unavailable, continue independent parser/runner work but mark enforcement blocked and keep legacy teardown locked. Ask for the specific missing access only after preparing the test setup. Do not downgrade the product to advisory feedback to clear this gate.

## 3. Phase 1 — freeze the contracts (Terra owns)

- [ ] Specify Rust input/output types and exact schema-7 JSON fixtures before parallel implementations. Reuse existing types where compatible; keep schema-6 behavior isolated during transition.
- [ ] Pin configuration examples and negative fixtures covering duplicate keys, invalid `on`, empty policy, unknown keys, invalid globs, and timeout values.
- [ ] Pin the aggregate truth table: all pass; ordinary violation; violation followed by execution error; config/input failure; untrusted policy; zero-selected change; selected checks left unrun.
- [ ] Define JSON encoding of unavailable exit status, invalid UTF-8 diagnostics, top-level errors, and truncation; define stable ordering and error precedence. Match existing public field conventions where useful.
- [ ] Audit current shell status classification and explicitly pin 126/127 and signal-style statuses. If implementation and spec wording differ, record the resolution rather than copying an accidental behavior.
- [ ] Pin environment retention, removal of inherited reserved variables, timeout override behavior, path normalization including symlink cases, and platform scope. Retain the spec's defaults; an inherited 0.4 override must not silently change v1 semantics.
- [ ] Define how approved scripts outside the candidate refer to candidate files through the root, without candidate-controlled PATH resolution replacing the evaluator or mandatory enforcement scripts.
- [ ] Choose the smallest transition dispatch: config version selects evaluator/ABI/output schema; unversioned supported 0.4 config retains its existing semantics until release contraction. Invalid/unknown versions never fall through to legacy parsing.
- [ ] Define error output when config is not parseable enough to select a version; choose an unambiguous transition rule and pin it in fixtures. Freeze explicit v1 invocation behavior for the enforcing consumer.

Write these decisions next to the executable fixtures or in a short contract note linked here. Do not create a generic protocol package. Freeze this gate before handing contracts to workers; subsequent changes require updating all consumers and fixtures in one coordinated slice.

**Gate:** A separate Luna reviews these contracts against spec §§5–8 before dependent implementation. Terra resolves findings. No agent is permitted to invent an extra event, optional acceptance tier, cache, retry, or bypass flag.

## 4. Dependency schedule

| Wave | Packet(s) | Dependency | Parallelism |
| --- | --- | --- | --- |
| 0 | Baseline and boundary proof | None | Terra; bounded read-only harness research can be delegated |
| 1 | Frozen contracts | Phase 0 findings | Terra + independent reviewer |
| 2 | A: config/selection; B: process execution | Contracts frozen | Two disjoint workers; Terra prepares CLI integration tests |
| 3 | C: evaluator/verdict | A + B reviewed | One worker; Terra integrates public exports |
| 4 | D: CLI/consent | C reviewed | One worker; Terra prepares reference integration |
| 5 | E: feedback adapter; F: migration inventory/report | D reviewed | Two workers with disjoint adapter/CLI ownership |
| 6 | G: reference acceptance; H: installer migration | D, Phase 0 proof; F for H | Terra owns G; one Luna owns H |
| 7 | I: contraction and docs | E–H proven; migration rehearsal | Serial teardown packets with review |
| 8 | Release verification | All packets reviewed | Terra serializes shared build tools; Luna reviews integration |

If a listed pair touches the same file, serialize it. The schedule is a dependency graph, not a requirement to keep all slots occupied.

## 5. Implementation packets

Paths below are starting locations in the current repository; Phase 0 must refine them to exact files. Tests belong with the existing relevant suite. Each packet can be several dispatches, with the listed gate between packets.

### A — versioned config and pure selection (Luna)

**Own:** Relevant `crates/ironlint-core/src/config/` files and config/selection tests; exclude files reserved for Terra's public exports.

- [ ] Add v1 parsing using the existing YAML dependency, including duplicate-key rejection at every mapping level; do not deserialize away duplicates before validation.
- [ ] Implement defaults, mandatory acceptance, optional change, strict fields and positive integer budgets.
- [ ] Implement pure selection from event and optional changed-path set: acceptance always selects all, unknown selects all change checks, known empty selects only unconditional change checks.
- [ ] Preserve bare-glob semantics; deletions need not exist; ordering is lexicographic and each check appears once.
- [ ] Prove v1 does not apply inline suppressions or legacy inheritance/steps; keep legacy parsing behavior intact in transition.

**Evidence:** Table-driven tests include a nonmatching indirect dependency at acceptance, rename endpoints, duplicate matches, and known-empty versus unknown. Gate: no process execution in the selector.

### B — bounded command execution (Luna)

**Own:** `crates/ironlint-core/src/engine/` execution files and focused process tests.

- [ ] Add the tree-reading v1 execution path with closed stdin and the frozen environment contract, reusing process management rather than creating another runner.
- [ ] Capture each pipe to 64 KiB, keep draining after the cap, and preserve explicit per-stream truncation flags.
- [ ] Enforce monotonic per-check and remaining invocation deadlines; preserve descendant cleanup and bounded drain completion even when descendants hold pipes open.
- [ ] Classify ordinary failure, unavailable shell/command, signals, and deadlines according to frozen fixtures.
- [ ] Keep legacy proposed stdin/env behavior isolated until contraction.

**Evidence:** Real short-lived child processes cover noisy simultaneous stdout/stderr, invalid bytes, a descendant retaining a pipe after its parent exits, signal/timeout, and deadline cleanup. Use generous wall-clock assertions around short configured budgets; no long sleeps or flaky millisecond equality tests. Gate: neither output memory nor pipe EOF can bypass the budget.

### C — evaluate once, aggregate honestly (Luna)

**Own:** Relevant `runner` and `verdict` files plus tests; depends on A/B APIs.

- [ ] Implement `evaluate(policy, root, event, changed_paths?)` with one execution per selected check in stable order.
- [ ] Continue ordinary violations; stop execution errors; list remaining selected checks as not run with reasons.
- [ ] Account for the total deadline across the invocation and classify incomplete evaluation as error even when earlier checks failed normally.
- [ ] Produce the frozen schema-7 outcomes and preserve transition schema-6 output for legacy consumers.
- [ ] Keep consent at the CLI boundary; pure load/read-only inspection does not execute or trust commands.

**Evidence:** One three-check test proves ordering, mixed-result precedence, and not-run reporting; another proves bulk matching starts one process per check. Gate: `pass` requires all required checks to have run successfully; no telemetry or formatting failure silently converts evaluation into success.

### D — CLI, local consent, and public surfaces (Luna)

**Own:** CLI argument and check/validate/schema command files and CLI tests; Terra owns shared wiring where another packet needs it.

- [ ] Add v1 event/root/repeatable-file behavior, validate paths without requiring deleted paths to exist, and reject file filtering for acceptance.
- [ ] Preserve outer codes 0–4 and expose every failure in the frozen machine format when JSON is requested, including pre-execution failures.
- [ ] Preserve local consent and changed-policy/script detection; never infer trusted CI mode from candidate-provided flags or environment.
- [ ] Ensure bare v1 `check` evaluates acceptance and legacy configuration dispatch remains explicit during transition.
- [ ] Update `validate`/`schema` and retained read-only consumers to understand v1; identify `explain`, `show-resolved-config`, `doctor`, telemetry, and watch compatibility rather than leaving hidden schema-6 assumptions.

**Evidence:** CLI tests exercise actual command invocation and JSON parsing, not just Rust constructors. Check malformed input, untrusted policy, inherited legacy env, and successful zero-selected feedback. Gate: a caller using the enforcing predicate cannot mistake `not_run` for acceptance.

### E — one verified feedback adapter (Luna)

**Own:** Only the selected `adapters/<harness>/` implementation and contract tests. Installer mutation belongs to H.

- [ ] Translate the audited completed-operation/batch event into `change` evaluation; collect deletion/rename paths and handle unknown paths conservatively.
- [ ] Return visible failures/errors plus reproduction command; keep edits and suppress routine success.
- [ ] Keep execution synchronous and bounded; label results superseded if newer edits are observed; do not promise a stable workspace snapshot.
- [ ] Remove proposal reconstruction from the new adapter path and do not call `gate-bash` there.
- [ ] Capture real payloads and delivery evidence with harness version/provenance. Record unsupported write paths and feedback-only capability.

**Evidence:** Live TDD red→repair→green, bulk edit, missing executable, and unsupported mutation followed by full acceptance. Gate: a README-only fixture directory or synthetic event cannot satisfy the one-verified-adapter release requirement.

### F — reviewable migration report (Luna)

**Own:** One migration command/helper and its fixtures, using existing config resolution; exact location/API chosen by Terra before dispatch.

- [ ] Inventory legacy checks, resolved inheritance, steps, suppression occurrences, stdin and per-file ABI references, lifecycle changes, and installed blocking surfaces.
- [ ] Produce a report and proposed v1 configuration/script content without overwriting config, approving trust, or changing installed hooks.
- [ ] Flatten inheritance with existing collision semantics; preserve command ordering when proposing scripts; flag unsupported shell/ABI transformations for manual rewrite.
- [ ] Treat scanning as a migration aid, not a proof: shell indirection and transitive scripts can hide dependencies. Mark unresolved checks and do not label a proposed migration complete while they remain.
- [ ] Map all checks to acceptance; suggest early change only with an explicit rationale. Report the loss of write blocking plainly.

**Evidence:** Representative inherited/steps config and a proposed-content rule yield a deterministic, reviewable report with explicit manual work. Gate: no automatic trust or executable command rewriting claimed safe merely because string matching found no dependency.

### G — reference acceptance integration (Terra owns; Luna can implement bounded tests)

- [ ] Replace Phase 0 stub evaluation with the reviewed CLI, fixed approved policy/evaluator, and exact candidate materialization.
- [ ] Implement one small enforcing consumer requiring exit zero, matching JSON version, `accept`, complete pass, and the expected required result set; fail closed on any mismatch.
- [ ] Keep publication/acceptance authority outside candidate execution. Bind success to the exact candidate through the selected platform's native mechanism; do not use schema 7 as a portable authorization token.
- [ ] Document exact installation, policy update approval, credential/permission separation, supported merge semantics, and explicit limitations.
- [ ] Run actual platform denial tests from Phase 0 with IronLint, including stale success, forged results, policy replacement, missing evaluator, and changed merge inputs where supported.

**Gate:** Evidence must name the denied operation and tested revision, not merely show an IronLint exit code. An ordinary workflow editable by the candidate is not an acceptable shortcut. If the existing platform cannot meet the contract, keep the enforcement gate blocked; do not add a new security product.

### H — explicit, reversible installation migration (Luna)

**Own:** Relevant `crates/ironlint-core/src/adapter/` installation files, CLI init/migration wiring assigned by Terra, and installation fixtures.

- [ ] Add dry-run reporting and explicit application of owned-entry changes using existing installer/uninstaller machinery.
- [ ] Replace/remove owned legacy Bash registrations before removing their executable path; preserve unrelated hooks, chains, settings entries, and user-modified content.
- [ ] Treat ownership ambiguity as manual migration with a concrete diff; never delete entire settings files to avoid parsing them.
- [ ] Stop default floor-hook installation. Retain an opt-in convenience hook only if existing machinery makes it small, with an explicit working-tree/partial-staging warning.
- [ ] For unsupported adapters, safely uninstall owned legacy blocking registrations during explicit migration and report missing replacement capability; do not fabricate post-edit support.
- [ ] Rehearse interruption/retry at each migration step and matched binary/config/adapter rollback; record only the minimal recovery information needed by existing install metadata.

**Gate:** Old hooks must still have a matching executable until they are migrated. Test a user-edited/chained hook and interrupted migration in a temporary home/repository. Never test against the developer's live installation.

### I — contraction and documentation (serial Luna packets; Terra integrates)

Unlock only after G passes and E/F/H migration rehearsals pass.

- [ ] Make v1 reject legacy config with actionable migration instructions; remove transition dispatch only after retained behavior has replacement coverage.
- [ ] Remove Bash-gate crate/dependencies/command, legacy preview stdin/ABI path, suppression execution, obsolete diff selection, and automatic floor installation where no retained consumer needs them.
- [ ] Search graph callers and unindexed config/scripts/docs before each deletion; remove dead tests, retain historical specs and transition evidence. Do not leave installer templates calling deleted commands.
- [ ] Update package/version metadata, README, generated schema/authoring skill source, init templates, doctor messages, CI wiring, and current repository guidance to match the final product.
- [ ] Freeze telemetry/watch/update features; retain compatible behavior with no schema-7 confusion, or explicitly remove an incompatible surface with a migration note. Do not redesign them.
- [ ] Keep unsupported adapters clearly unsupported/capture-pending and free of active references to removed commands after migration.
- [ ] Refresh Graft after substantial code changes and clean only build artifacts produced by this work.

**Gate:** No default install advertises unavoidable local enforcement, invokes `gate-bash`, or promises rejected content never reaches disk. Deletion is authorized only within the implementation scope and must preserve unrelated user changes noted in Phase 0.

## 6. Release evidence and completion

Keep evidence references in this table as work completes; unchecked means incomplete, not waived.

| Spec §14 gate | Owner | Required evidence |
| --- | --- | --- |
| 1 TDD | E + G | Live editable red state and external acceptance denial; green accepted |
| 2 Violation | G | Protected operation denied for nonzero required command |
| 3 Broken evaluator | D + G | Missing binary, timeout, signal, invalid config, consent failure, malformed JSON deny |
| 4 Wrong candidate | G | Stale revision cannot pass; partial staging cannot authorize another tree; merge-input test for supported merge boundary |
| 5 Policy bypass | G | Approved policy/evaluator isolation and forged-result denial |
| 6 Unsupported write | E + G | Missed feedback still caught by full acceptance |
| 7 Bulk edit | A + C + E | One process per selected check, independent of path count |
| 8 Selection | A + C | Full acceptance despite nonmatches/deletions; empty policy rejected |
| 9 Resource limits | B + C | Bounded capture, total deadline, pipe-holding descendant cleanup |
| 10 Migration | F + H + I | Owned-entry preservation, interruption recovery, no stranded hooks |

- [ ] Run `rtk cargo test`, `rtk cargo clippy --all-targets -- -D warnings`, `rtk proxy cargo fmt --check`, `rtk proxy bash scripts/ci-coverage.sh`, and `rtk proxy bash scripts/ci-adapters.sh` against the integrated tree, adapting only commands demonstrably changed by the implementation.
- [ ] Meet ≥80% per-file Rust region coverage and complexity limits; do not lower thresholds or exclude new code to pass.
- [ ] Run a separate Luna integration review focused on authority bypass, schema consumer mismatches, and installation teardown; Terra reproduces material findings and resolves them.
- [ ] Inspect final generated artifacts and docs; confirm release metadata and installer content correspond to the tested tree.
- [ ] Report passed, failed, and unavailable gates separately. Never describe a mock boundary test as live enforcement evidence.

Completion means all required evidence exists and no required gate is open. Publishing is a separate authorized action. If external authority or live harness capture is unavailable, deliver the implemented subset and exact blocker without declaring v1 complete. Do not compensate by expanding shell classification, adding a local daemon, or weakening the guarantee.

## 7. Plan pressure test

### Decision Inventory

| # | Decision | Explicit or implied? | Where |
| --- | --- | --- | --- |
| 1 | Terra owns contracts and integration; bounded Luna workers | Explicit | §1 |
| 2 | Shared checkout with exclusive file ownership; serialized Cargo | Explicit | §1 |
| 3 | Platform/harness selection precedes dependent work | Explicit | §2 |
| 4 | Authority proof precedes teardown | Explicit | §2, G, I |
| 5 | Versioned config/ABI/JSON transition, then removal | Explicit | §3, I |
| 6 | Golden fixtures bind multiple consumers to one contract | Explicit | §3 |
| 7 | Pure selection and one serial command per check | Explicit | A–C |
| 8 | Synchronous feedback without acceptance state/cache | Explicit | E |
| 9 | Consent remains separate from acceptance authority | Explicit | D, G |
| 10 | Candidate execution cannot publish its own trusted result | Explicit | §2, G |
| 11 | Migration report before explicit owned-entry changes | Explicit | F, H |
| 12 | One evidence table links tests rather than duplicating status stores | Explicit | §6 |

### Irreversibility Triage

| Rows | Classification | Isolation |
| --- | --- | --- |
| 1–4, 6–9, 12 | TWO-WAY | Internal execution choices; no published commitment before evidence |
| 5 | ONE-WAY | Versioned fixtures and transition dispatch; matched-version rollback before contraction |
| 10 | ONE-WAY | Exact supported deployment documented and tested before any enforcement claim; no authority configuration changed by plan execution alone |
| 11 | ONE-WAY | Dry-run diff, explicit application, owned-entry preservation, interrupted migration recovery, matched rollback |

### Fork Audit

- **One service vs split**, rows 1–2: chosen “None of the above is demonstrated today” is met by one workspace/executable; opposing “separate teams own them and already coordinate through a contract” is not met by temporary implementation workers; satisfied YES.
- **Migration strategy**, rows 5/11: chosen “Real data exists in production” is met by existing installed configs/hooks; opposing “pre-launch ... without anyone noticing” is not established; satisfied YES through transition then contraction, without unnecessary dual-writing of verdicts.
- **Normalize vs denormalize**, rows 6/12: chosen “duplicated fact feeds a correctness decision” applies to schema/acceptance semantics, so fixtures and linked evidence are authoritative; opposing “read path is measured-hot” is not established; satisfied YES.
- **Batch vs stream**, rows 7/8: chosen “No freshness SLA is written down” is met; opposing “consumer must react to individual events as they occur” partially applies to edit diagnostics, but E explicitly chooses completed-operation batches and rejects a watcher because full acceptance independently checks correctness; satisfied YES with stated tradeoff.
- **Sync call vs queue/background job**, rows 7/8: chosen “caller must abort if the work fails” applies to acceptance; opposing “external service whose latency/availability you don't control” can apply to arbitrary check commands, but B/C/E explicitly choose bounded synchronous checks and reject automatic retry/background completion because acceptance needs a completed verdict and command side effects are not assumed idempotent; satisfied YES with stated tradeoff.
- **Config vs code**, rows 5/6: chosen config condition “operator must change the value without a deploy” applies to repository rules; opposing code condition “invariant other code silently assumes” applies to verdict interpretation, which §3 freezes in code/fixtures rather than configurable semantics; satisfied YES for these distinct responsibilities.
- No additional fork match: rows 3, 4, 9, 10; these concern capability evidence and authority ownership rather than a storage/service choice.

### Corner Scan

- Unmigratable schema: ABSENT — §3 and I require explicit version transition and consumer fixtures before contraction.
- Side effects without idempotency keys: ABSENT — no automatic check retry exists, and H rehearses repeated/interrupted owned-entry migration without repeating destructive replacement.
- Test-hostile boundaries: ABSENT — A provides pure selection and G separately tests the protected operation.
- Auth/tenancy bolted on later: ABSENT — §2 makes acceptance authority a prerequisite rather than a post-release retrofit.
- Unbounded growth: ABSENT — B caps capture and the plan adds no persistent runtime history store.
- Hidden fan-out: ABSENT — C runs once per check under an invocation deadline and §1 caps worker concurrency.
- Shared mutable state across workers: ABSENT — §1 enforces single ownership of files and serialized shared build operations.
- Clock in the logic: ABSENT — B uses monotonic budgets with bounded real-process tests, without calendar-time state.
- Hard external coupling, no failure mode: ABSENT — §2/G keep unavailable authority tests blocked and deny acceptance on missing or malformed evaluation.
- “Pagination later”: ABSENT — no list endpoint or growing query API is introduced.

### Pre-Mortem

1. **The candidate forges its own success.** Mechanism: a check obtains the CI publication credential or rewrites the verdict artifact; §2/G forbid that privilege arrangement and require an actual forged-result denial test before teardown.
2. **The upgrade leaves a mixed installation.** Mechanism: interruption after binary replacement leaves an old registration calling removed `gate-bash`; H requires interruption/retry tests and matched rollback, and I stays locked until that rehearsal passes.
3. **Feedback disappears after a harness update.** Mechanism: a completed-edit callback no longer arrives or diagnostics no longer reach the agent; E requires pinned live evidence and unsupported-path reporting, while G always performs fresh acceptance independent of callbacks, so correctness does not depend on event delivery.

### Verdict

**VERDICT: PASS** for this staged implementation plan, not for untested deployment capability.

Findings that survived the discard rule: NONE; the three pre-mortem mechanisms are covered by mandatory proof/recovery gates in §2/G, H/I, and E/G respectively.

DISCARDED: NONE.

Required edits: NONE. Phase 0's platform/harness evidence and Phase 1's exact contracts are required execution deliverables, not permission to invent missing integration capabilities. Failure to produce them blocks the dependent work as specified above.
