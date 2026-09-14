# IronLint v1.0 — deterministic acceptance gates

**Updated:** 2026-09-14
**Status:** Current v1 release contract. Core/CLI evaluation is implemented;
external acceptance, live feedback, and installation/code cleanup remain open.
**Scope:** A breaking release. No legacy config execution, schema-6 compatibility,
automatic conversion, migration-report command, or coordinated rollback is required.
**Implementation:** [current architecture](../docs/architecture.md).
**Execution:** [active plan](../plans/2026-09-05-ironlint-v1-implementation.md).

## 1. Product decision

IronLint is a deterministic acceptance gate for AI-authored changes.

An agent may create incomplete, failing intermediate states in its working
workspace. It may not promote a change through an enforced acceptance boundary
unless every required check succeeds. The decision comes from commands, not a
model's interpretation, cooperation, or agreement.

Early feedback helps the agent repair its work. It does not replace the gate.
TDD's red phase is permitted; accepting a change while required tests fail is not.

The product promise is conditional on a concrete integration:

> At a declared, controlled acceptance boundary, IronLint denies acceptance
> unless the required checks complete successfully against the candidate being
> accepted under the selected developer policy.

IronLint owns evaluation. The integration owns the operation that evaluation
authorizes. Neither component alone supplies the end-to-end guarantee.

The v1 reference acceptance boundary is repository acceptance of an exact
revision through a required CI result. Local adapters provide early feedback.
A harness may also provide an enforced local boundary if it meets section 4.
No current harness is presumed to meet that contract.

## 2. Goals and non-goals

### Goals

- Make acceptance independent of model alignment with static rules.
- Permit TDD, incomplete edits, multi-file refactors, and local failing commits.
- Run ordinary CI-style commands against files on disk.
- Deliver useful diagnostics shortly after supported edit operations.
- Keep rule evaluation independent of harness tool names and payloads.
- Distinguish a violation from inability to evaluate, while denying acceptance
  in both cases.
- Keep one executable, a small configuration model, and thin integrations.

### Non-goals

- Prevent every invalid byte from ever reaching any disk.
- Prevent an agent from writing or committing within its disposable workspace.
- Infer whether arbitrary shell commands mutate protected paths.
- Provide a sandbox, permission system, merge service, filesystem watcher,
  transactional patch engine, or general workflow scheduler.
- Prove arbitrary commands correct, deterministic, read-only, or non-malicious.
- Certify that the configured checks exhaustively express developer intent.
- Block writes by default or implement universal proposal simulation.
- Automatically fix, roll back, stash, stage, or restore user files.

If a deployment requires prohibited content never to enter a protected
workspace, its harness must isolate candidate edits and control application to
that workspace. Building that machinery is outside v1.

## 3. Terms and state transitions

**Working workspace:** Mutable state in which the agent edits and runs tests.
**Candidate:** The exact source revision or controlled snapshot submitted for acceptance.
**Policy:** The developer-selected configuration and the enforcement code it relies on.
**Acceptance boundary:** A named operation whose owner can withhold its effect.
**Feedback:** Evaluation that supplies diagnostics without authorizing acceptance.

```text
edit -> working state -> optional feedback -> more edits
                |
                +-> submit candidate -> evaluate required checks
                                           |
                                  pass ----+---- failure/error
                                   |                  |
                           accept same candidate    deny; allow repair
```

A local commit is not necessarily acceptance. A harness's final message is not
acceptance unless the harness controls an explicit completion operation and
cannot complete it through another path. A passed local invocation is not an
authorization token that may be replayed against later edits.

No persistent approved/dirty state machine or reusable local verdict cache is
required. A fresh acceptance attempt evaluates its candidate again.

## 4. Enforcement contract and authority

An integration may advertise **enforced acceptance** only if all of these hold:

1. It identifies the operation being protected: for example, updating a
   protected branch or completing a controlled workspace promotion.
2. Every route to that operation requires evaluation. A missing invocation,
   disabled integration, malformed response, timeout, or unavailable executable
   denies the operation.
3. The candidate evaluated is the candidate accepted. The owner holds it stable
   or rejects/re-evaluates after a change. A working-tree result cannot authorize
   a different staged tree, commit, or merge result.
4. The agent cannot weaken the gate, change the selected policy, forge its
   accepted result, or exercise credentials that bypass the boundary.
5. The required evaluator and enforcement scripts come from an approved source.
   Executing an agent-modifiable script that can simply return zero does not
   enforce the intended rule against that agent.
6. Candidate code and check execution cannot tamper with the authority that
   records the verdict or performs acceptance. Isolation is the boundary
   owner's responsibility, especially when checks execute candidate code.

The guarantee covers the selected checks and their declared inputs. A policy
can deliberately delegate behavior to candidate files, but must not describe
that delegation as protection against changes to those same checks.

Read-only flags on files owned by the same unrestricted principal are not an
independent permission boundary. A local trust hash is execution consent and
change detection, not an unforgeable acceptance authority.

An integration that does not meet all six requirements must describe itself as
feedback or a local convenience gate. This is a capability distinction, not a
user-selectable switch that manufactures stronger permissions.

### Reference CI integration

- A trusted job selects the required policy and evaluator independently of
  unreviewed candidate modifications.
- It materializes the exact candidate revision and runs acceptance evaluation.
- Only a successful, complete result satisfies the required acceptance check.
- The repository boundary associates that result with the evaluated revision
  and does not accept an older result for a new candidate.
- Changes to policy, enforcement scripts, and boundary settings require the
  relevant developer approval outside the agent's authority.
- If acceptance concerns a merged result, evaluate that result; a check of the
  feature branch alone does not establish properties of the merge.

The implementation must document the exact supported repository setup and test
its denial paths. Merely running IronLint somewhere in CI is insufficient.

## 5. Core abstraction

```text
evaluate(policy, root, event, changed_paths?) -> verdict
```

The evaluator parses configuration, selects checks, executes commands, and
returns structured outcomes. It does not perform acceptance itself.

There are two events:

| Event | Selection | Effect of failure |
| --- | --- | --- |
| `change` | Checks opted into early feedback, filtered by changed paths when known | Report diagnostics; retain edits |
| `accept` | Every configured check, irrespective of changed paths | Caller must deny acceptance |

All checks are acceptance requirements. Opting into `change` adds an early run;
it never removes the acceptance run. There is no feedback-only rule tier in v1.

`files` is an early-feedback trigger filter, not a command sandbox or a complete
dependency graph. Acceptance runs every check because a changed manifest,
deleted file, configuration change, or indirect dependency can invalidate a
check without matching its usual source globs.

Known empty changed paths and unknown changed paths are distinct. Empty means
no file-filtered change checks are selected. Unknown runs all change checks.
Checks without `files` always run for their declared event.

Each selected check runs once per invocation, in lexicographic check-ID order.
The command owns file iteration. The core does not fan a project command out
once per matched file. The same command sees the same on-disk input model in
both events.

## 6. Configuration

The implemented v1 format:

```yaml
version: 1

execution:
  timeout_secs: 30
  total_timeout_secs: 300

checks:
  architecture:
    files: ["src/**", "scripts/check-architecture"]
    on: [change, accept]
    run: ./scripts/check-architecture

  tests:
    run: ./scripts/test
```

- `version: 1` is required and denotes this configuration contract.
- `checks` is a nonempty mapping of stable IDs to checks.
- A check has required nonempty `run`, optional `files`, and optional `on`.
- `on` defaults to `[accept]`. Valid values are `[accept]` and either ordering
  of `[change, accept]`; duplicates, unknown events, and omission of `accept`
  from an explicit list are errors.
- `files` accepts a nonempty glob or nonempty list of globs. Existing bare-glob
  matching semantics remain: `*.rs` matches at any depth.
- `execution` is optional. Defaults are 30 seconds per check and 300 seconds
  per invocation. Values must be positive integers; zero is invalid.
- Unknown keys are errors. Duplicate mapping keys are errors.

There is no `steps`, `extends`, severity, conditional expression, per-rule
blocking switch, or inline suppression in the v1 format. Put command sequences
in scripts and explicit policy exceptions in reviewed check implementations.
Source comments cannot cause the core to skip an acceptance requirement.

The example assumes the scripts are developer-approved for the deployment.
A trusted integration must resolve their provenance as required by section 4;
their location in the candidate is not itself evidence of approval.

## 7. Execution and command ABI

- Run commands with `sh -c` from the supplied root, retaining the current
  supported platform scope. No new cross-platform shell guarantee is added.
- Stdin is closed. Commands read the actual evaluated tree.
- Retain only `PATH`, `HOME`, `LANG`, `TZ`, `TMPDIR`, and `LC_*` from the
  parent environment; add the reserved variables below. The integration must
  separately isolate filesystem access and publication credentials.
- Supply `IRONLINT_ROOT`, `IRONLINT_EVENT`, and `IRONLINT_BIN`.
- Do not supply legacy proposed-content, temporary-file, or per-file variables
  under v1 semantics. Clear inherited reserved `IRONLINT_*` values first.
- Changed paths select feedback checks; they are not passed as an unsafe
  whitespace-delimited command argument list. Scripts select their own inputs.
- Run serially. No daemon, automatic retries, dependency graph, or cache.
- Enforce the smaller of the remaining invocation budget and per-check budget.
- Retain process cleanup and bounded pipe-draining behavior. Cap captured
  stdout and stderr at 64 KiB each per check, continue draining excess bytes,
  and explicitly mark truncation.
- Continue after an ordinary check failure to gather other diagnostics while
  budget remains. Stop on execution failure or exhausted budget and report
  remaining selected checks as not run.

Acceptance commands must inspect the candidate, not silently repair source.
Build artifacts in designated scratch locations are permitted. The core is
not a sandbox: the integration must ensure source mutations cannot turn the
evaluated candidate into a different accepted candidate.

Command success means the command returned zero. Ordinary nonzero exits are
violations; unavailable execution, signal termination, and timeout are errors.
Classify exits 126/127 and all exits at least 128 as execution errors,
along with signal termination and timeout. A command's diagnostics never change its verdict.

Pinned tools and controlled inputs are necessary for repeatable command
results. IronLint guarantees deterministic dispatch and classification, not
reproducibility of arbitrary external programs.

## 8. CLI and verdict contract

Implemented v1 evaluation surface:

```text
ironlint check                         # accept evaluation, all checks
ironlint check --event accept
ironlint check --event change --file src/example.rs
ironlint check --event change          # unknown paths: all change checks
ironlint check --config PATH --root PATH --format json
ironlint validate
ironlint schema
```

`--file` is repeatable and accepted only for `change`. Relative paths resolve
against `--root`; paths must normalize within that root. Do not require a path
to exist: deletions and both sides of renames are relevant trigger inputs.
NUL bytes and paths escaping the root are input errors. This is input validation,
not containment of arbitrary commands.

An invocation without `--file` means unknown changes, not an empty set. A
feedback adapter with a known empty set may omit invoking the evaluator if it
has no unconditional work, or call the core with the explicit empty set.

No acceptance `--check`, diff filtering, or force-pass flag is provided. A
caller can evaluate another config, but the acceptance owner fixes which policy
is authoritative; the agent cannot select a weaker one for the boundary.

Keep outer exit numbers: `0` successful evaluation, `1` config/input error,
`2` check violation, `3` execution/incomplete error, `4` untrusted local policy.
For mixed results, errors take precedence over violations. Preserve all
completed check results in JSON.

V1 JSON uses schema 7 and includes:

- `schema`, `event`, and aggregate `status`;
- ordered check results with ID, outcome, exit status where available, captured
  stdout/stderr, and truncation flags;
- selected checks not run and their reason;
- top-level input, consent, or execution errors where applicable.

Aggregate outcomes are `pass`, `violation`, `error`, and `not_run`. A change
invocation selecting no checks may exit zero with `not_run`; it must not say the
workspace passed. Acceptance requires a nonempty policy and every check passing.
The exact implemented shape is documented in [Verdict JSON](../docs/reference/verdict-json.md)
and exercised by core verdict and CLI v1 tests.

An enforcing caller requires exit zero, valid matching-version output, event
`accept`, and a complete `pass`. It owns candidate/result binding; schema 7 is
not a signed authorization receipt. A missing or malformed result always denies
acceptance. `IRONLINT_FAIL_CLOSED_ON_INTERNAL` cannot weaken this behavior.

## 9. Adapter contract and early feedback

Adapters translate supported events and return diagnostics to the agent. They
do not understand language rules or reconstruct edits in the default path.

For a supported completed edit operation or batch:

1. Collect known changed paths, including deleted paths and rename endpoints.
2. Invoke `change` evaluation against the working tree.
3. Attach failures to the tool result or another documented agent-visible
   channel, naming the check, diagnostic, and reproduction command.
4. Leave the edit in place so the agent can repair it.

Failures and evaluation errors are visible feedback, not vetoes of subsequent
repair writes. Local consent failure prevents running unapproved commands but
does not freeze editing. Suppress routine successful output.

Do not launch detached work or retry automatically. Bound feedback by the
configured invocation budget. If the adapter observes a newer edit while a
check runs, label the earlier result superseded. Feedback carries no snapshot
guarantee in a concurrently edited workspace.

Each adapter publishes a tested capability record: harness version, observed
events/tools, feedback delivery mechanism, unsupported paths, and whether it
controls any acceptance operation. Synthetic tests alone cannot establish
live support. Preserve provenance-stamped captures and explicit capture-pending
status for integrations not yet verified.

Missing events mean missing early feedback. A fresh full acceptance evaluation
provides the required final check; adapters must not infer acceptance from a
history of successful edit checks.

## 10. Pre-write checks and local Git hooks

Generic pre-write blocking is not part of the v1 required implementation.
Existing preview behavior is removed as part of the breaking release. Ordinary CI commands should not need a separate stdin implementation.

A future pre-write feature must be explicitly scoped to commands consuming the
proposal, exact tool semantics, and a harness-controlled write operation. It
must not claim project-wide correctness from a single-file preview. This is
deferred rather than simulated through temporary writes and restoration.

A local Git hook may invoke acceptance evaluation as a convenience, but is
opt-in and not installed as an unavoidable floor. If commands inspect the
working tree, its documentation must say so; partially staged content is not
the same input. Exact staged-tree materialization is outside the minimum v1.

Bypassing that hook does not bypass a separately enforced repository acceptance
boundary. IronLint does not classify shell commands to prevent the bypass.

## 11. Execution consent and policy updates

Retain local explicit consent before automatically executing unfamiliar
repository commands. Preserve detection of changes to approved configuration
and managed scripts. Consent does not protect all transitive dependencies and
does not imply that code is safe to execute without isolation.

In CI or another enforced boundary, the owner supplies approved policy through
its trusted setup. Trust bootstrapping is never selected by the candidate or
inferred from candidate-provided environment flags. The implementation may
reuse the existing consent store instead of adding another policy service.

Policy changes are ordinary proposals requiring developer approval at the
external boundary. Checks execute under the currently selected approved policy
until the owner explicitly adopts the replacement. No local shell blocklist
tries to distinguish an authorized human from an agent sharing their account.

## 12. Architecture and implementation limits

Retain the Rust core runner and thin CLI where they directly implement this
contract. Place harness installation and event translation outside the pure
evaluation layer. Do not reorganize crates solely for architectural symmetry.

Delete the Bash-gate crate after safe removal of its installed registrations. Retain useful config
validation, scope matching, process execution, consent, and diagnostic code.
Remove legacy features from the v1 execution path rather than layering another
policy engine over them.

Do not build a local promotion service to make the first release possible.
Ship one tested external acceptance integration and one tested feedback adapter.
Additional harnesses may remain explicitly feedback-only or capture-pending.

## 13. Breaking release and removal

Backward compatibility is not a release requirement. Remove unversioned config
execution, schema-6 consumers, proposal stdin/per-file ABI, inline suppression,
unused diff selection, the Bash classifier, and automatic floor-hook installation.
Reject old configs with a clear unsupported-format error and current authoring
instructions. Do not build automatic conversion, a migration-report command, or
coordinated binary/config/adapter rollback.

Use existing installer ownership records to remove or replace IronLint-owned
registrations before deleting the commands they call. Preserve unrelated hooks,
chains, settings, and user edits. Ambiguous ownership requires a concrete manual
cleanup instruction; never delete an entire settings file. Exercise cleanup in a
temporary home/repository, including chained hooks and a repeated invocation.
This prevents stranded installations without preserving old execution semantics.

Finish and prove the external acceptance integration and one feedback adapter
before presenting the new installation as complete. Unsupported adapters must
be clearly identified and have no active registrations calling deleted commands.
`init`, generated authoring instructions, CLI help, examples, and current guidance
must describe the implemented v1 behavior when the release ships.

Freeze telemetry/watch/self-update expansion. Retain those surfaces only where
useful under v1; otherwise remove them with a release note. No redesign is required.
Keep current architecture and the active plan up to date; remove obsolete planning
and design documents instead of maintaining a historical documentation tree.

## 14. Release acceptance tests

The following are release gates, not optional demonstrations:

1. **TDD:** Write a failing test, receive feedback, edit the implementation, and
   pass. The red state remains editable; acceptance during red is denied.
2. **Violation:** A required command exits nonzero; the protected acceptance
   operation does not occur, even if the agent requests it directly.
3. **Broken evaluator:** Missing executable, timeout, signal, invalid config,
   untrusted local policy, and malformed JSON cannot authorize acceptance.
4. **Wrong candidate:** Change the candidate after validation; the old result
   cannot authorize it. Cover partial staging and changed merge inputs in the
   reference integration's supported boundary.
5. **Policy bypass:** Candidate changes to config, scripts, hook files, or
   apparent check results cannot weaken the external acceptance requirement.
6. **Unsupported write:** A mutation outside the feedback adapter's coverage
   still fails the subsequent full acceptance check when it violates policy.
7. **Bulk edit:** A batch touching many matching files runs each check once.
8. **Selection:** Indirect changes, deletions, and nonmatching paths never omit
   acceptance checks. Empty acceptance policy is an error.
9. **Resource limits:** Chatty commands and descendant processes cannot defeat
   the documented output/deadline bounds. Truncated diagnostics are marked.
10. **Installation cleanup:** Removing IronLint entries preserves unrelated hooks and does
    not strand a legacy fail-closed adapter calling a missing command.

Core changes retain the repository's regression-test, separate code-review,
clippy, and per-file region-coverage requirements. Adapter claims require live
contract evidence. A failed enforcement test blocks an enforcement claim, not
merely a documentation checkbox.

## 15. Success and stop condition

The release succeeds when an agent can perform a normal TDD/refactoring session
without gate-induced edit deadlocks, while a failing or unevaluable candidate
cannot cross the tested acceptance boundary regardless of the agent's response.

Measure correction time, feedback interruption cost, and enforcement failures.
Blocked-write count is not a success metric.

If no usable integration can supply the authority contract, do not ship v1 as
a hard gate. Narrow supported deployments to the reference external boundary.
If the runner and feedback adapters add no practical value over directly running
the same scripts there, stop expanding IronLint rather than rebuilding a security
platform to justify it.
