# Durability, flexibility, and simplification

## Resume here

- **Authorization:** the user requested implementation of this plan with
  sub-agents, including the per-check timeout follow-up.
- **Status:** D0–D9 complete as of 2026-09-30. All implementation batches were
  independently reviewed; findings were fixed with failing regressions and
  independently rechecked. The final gates and cleanup are complete.
- **Next packet:** none. Final verification passes 516 Rust tests across 30
  suites, 37 Pi tests, TypeScript checking, strict all-target Clippy, formatting,
  and both local Docker suites. Region coverage is 93.93% overall and at least
  80% in every Rust file. Rust 1.88 builds and passes the same 516 tests.
  Windows production code compiles; native Windows execution remains unqualified.
- **Baseline:** the completed [v1 release plan](2026-09-05-ironlint-v1-implementation.md)
  records verification of `4828bfcd1811a1ef51c728bb32b977005d952713`.
  The review observed HEAD `e23b18c0e7e629b5a339d4391ef8cec696bed0ae`, a subsequent
  documentation commit. Initial changes were the planning documents and the
  user-managed temporary removal of the project hook; no source changes.
- **Evidence limitation:** the review inspected local source. An installed legacy
  hook rejected shell commands with `bash-gate failed`, so the findings are not
  fresh test results. Do not treat the earlier release results as validation of
  these changes. The user removed the obsolete installation; command execution
  resumed. Fresh baseline: Rust/Cargo 1.96.1, Node 24.19.0, 397 tests passing
  across 24 suites. Use the repository `target/` explicitly because the host
  default target directory is outside the writable workspace. Docker 29.4.0 is
  available with approved access to its local daemon.
- **Contract:** [v1 specification](../specs/2026-09-05-ironlint-v1-design.md).
  [Architecture](../docs/architecture.md) describes implemented behavior; the
  decisions below describe proposed behavior until their packets are complete.
- **Completion:** integrated fixes, the timeout extension, updated documentation,
  separate agent reviews, graph refresh, cleanup, and local validation gates are
  finished. Tagging and publishing remain separately authorized actions.

## Scope and design decisions

Keep ordinary shell commands, serial dispatch, mandatory complete acceptance,
known/unknown change paths, CLI exits 0–4, and schema-7 JSON. Keep Pi as the sole
installable adapter and retain ownership-aware removal of legacy installations.
The optional Git hook continues to inspect the working tree; exact staged-tree
materialization and repository enforcement belong to other work.
The subsequent [security fixes](../docs/security/2026-10-01-review-findings.md)
add a staged/working mismatch guard while retaining that local-hook boundary.

Use the existing two crates. Consolidate policy and filesystem behavior through
small shared modules and value types. Keep harness installation outside the
evaluation module; no new service, scheduler, cache, policy expression language,
generic executor plugin framework, or crate extraction is part of this plan.

Preserve the current consent-store shape and folded policy/script hash framing.
For unchanged bytes and paths, existing consent must remain valid. Parsing and
evaluation use the captured policy bytes, and pre-check/post-run verification
remains required. This work does not close the documented window in which a
check rewrites and executes a script inside its own shell; OS isolation remains
the integration owner's responsibility.

The policy extension adds optional check-level `timeout_secs` to `version: 1`.
Existing policies retain their behavior. Older binaries reject that unknown
field rather than silently ignore it. Document the minimum supporting release
when D6 lands; adopt new fields only after updating the evaluator and adapters.
Removing the field restores compatibility. Schema 7 does not change.

Expose a validated, read-only v1 policy and an immutable policy snapshot from
core. Local consent approval wraps the snapshot at the CLI boundary. Add the
new evaluator entry point without breaking the current public
`evaluate_v1(&ApprovedPolicy, ...)` caller contract: retain a thin forwarding
wrapper until a separately scoped breaking release. Both entry points use one
implementation and the same drift checks.

Configuration budgets belong to the policy. Adapter safety ceilings belong to
the adapter and have separate meanings; do not duplicate the policy timeout in
TypeScript. Operators change policy budgets through reviewed configuration and
renew consent. Runtime safety constants stay in code and are tested.

## Packets and dependencies

| ID | Work | Depends on | Batch | Status |
| --- | --- | --- | --- | --- |
| D0 | Establish baseline and reproduce defects | None | Preparation | Complete |
| D1 | Propagate hook and updater failures | D0 | A | Complete |
| D2 | Share safe file creation/replacement | D1 | A | Complete |
| D3 | Report stale installs and incomplete evaluations | D2 | A | Complete |
| D4 | Share policy model and separate consent from evaluation | D3 | B | Complete |
| D5 | Carry absolute deadlines and stream script verification | D4 | B | Complete |
| D6 | Add per-check timeout override | D5 | B | Complete |
| D7 | Bound and cancel Pi subprocess runs | D6 | C | Complete |
| D8 | Replace the installation TUI with plain confirmation | D3 | C | Complete |
| D9 | Verify the integrated tree and update evidence | D7, D8 | Final | Complete |

The ordering stabilizes user-visible failures before moving shared abstractions.
After batches A, B, and C, request a separate agent review scoped to the changed
behavior and its regressions. Fix findings before starting the next batch.

### D0 — Establish the baseline

Record HEAD, working-tree changes, toolchain versions, and the available test
environment. Reuse the graph cards and refresh stale spans before tracing
callers. The graph's concept nodes still name removed execution paths; source
and the current contract take precedence over those stale summaries.

Use a supported execution environment for validation. Document any hook blocker;
do not disable live hooks, modify the developer's trust store, or claim a passing
baseline while execution is unavailable. Installation and adapter regressions
must use temporary HOME/config/repository directories.

Start each bug packet with a regression that fails on the unchanged implementation.
For new behavior, pin its acceptance test before implementation. Capture focused
failure evidence in the packet; keep full logs in test output.

**Done when:** the baseline and blockers are recorded and the first D1 regression
has been observed failing. Unavailable runtime evidence leaves this packet open.

### D1 — Propagate shell failures

**Source:** `hook_block` in
`crates/ironlint-cli/src/commands/init/git_hook.rs:179`; `run_installer` in
`crates/ironlint-cli/src/commands/update.rs:90`.

Make a nonzero IronLint acceptance result exit the hook immediately while
preserving successful execution of any user-owned suffix. Test exits 1–4 with a
successful footer after the managed block, without relying on `set -e` in the
surrounding hook. Continue preserving the user's prefix/suffix bytes on reinstall
and uninstall. Retain binary-path quoting and the opt-in working-tree semantics.

Keep the updater synchronous because the caller needs completion status for this
explicit command; queued installation would add persisted jobs and change return
semantics. Separate installer download from execution. Execute only a fully downloaded
temporary installer, check both subprocess outcomes, and remove the temporary
file on every path. Keep the existing installer URLs and receipt eligibility.
Use a 60-second download cap within a 300-second overall update deadline; do not
automatically retry installation. Windows fetch/invocation errors must also
produce failure. Reuse supported process cleanup primitives where useful;
extract a small helper only if there are actual shared callers.

**Regressions:** failed download plus a shell that would otherwise exit zero;
partial download; downloader/installer missing or nonzero; downloader/installer
hang; successful installer; no-receipt path; hook failure followed by `exit 0`;
successful hook followed by an ordinary user command. Fake executables and
receipts must be isolated; successful-update tests must not contact a release
server or replace the developer's binary. Inject short test deadlines through a
private seam so the suite does not wait for production limits.

**Done when:** failures return nonzero with an actionable reason, no partial
installer runs, and the generated hook is exercised as a script in regressions.

### D2 — Consolidate filesystem writes

**Source:** `adapter/materialize.rs:30` (`atomic_write`), `trust/store.rs:88`
(`write_store`), `init/git_hook.rs:241` (`write_exec`), and `init/mod.rs:105`
(`classify_existing`) / `:123` (`scaffold_config`).

Keep the replacement helper under roughly 200 production lines; resource locking
and ownership decisions remain in callers. Create one internal replacement helper using an exclusively created sibling
temporary file, complete writes, flush/sync, and atomic publication on supported
platforms. Temp-name collisions retry exclusive creation rather than truncate an
existing temp file. Cleanup uses an ownership-scoped guard. Preserve existing
permissions for replacements. Explicit Git-hook installation adds owner execute
permission while retaining other existing mode bits, including on byte-identical
reinstall; new hooks use executable permissions and uninstall preserves modes.
Report publication/durability errors precisely, including when bytes were
published but a subsequent directory sync failed, so retries can recover.

New policy scaffolding uses exclusive creation at the final path. Refuse a
symlinked `.ironlint.yml`, including a dangling symlink, and handle a file created
between classification and creation without overwriting it. An interrupted new
write may leave an incomplete regular file: report the error, never bless partial
bytes, and preserve it for explicit repair on retry. Retain the byte-identical
baseline recovery path. Explicit config loading/trust continues to use its
existing canonical-path semantics.

Route adapter artifacts/sidecars/settings, trust-store replacements, and Git-hook
updates through the helper. Reuse existing ownership checks before mutation.
Atomic replacement does not prevent lost read-modify-write updates: serialize
IronLint mutations of the same settings file, hook, or artifact/sidecar resource
group and re-read state under that lock. Retain consent-store locking. Where an
operation needs multiple locks, acquire canonical lock paths in a stable order.
Human/editor writes remain outside those locks; describe that boundary honestly.

Retry after publication or sidecar failure must inspect both artifact and
ownership metadata: byte-identical content alone cannot short-circuit metadata
repair. A retry must not adopt a foreign artifact or erase user edits. Keep the
current supported platform scope and Windows compile-only qualification.

**Regressions:** simultaneous replacement writers; two settings edits both
survive; failure before publication leaves the original bytes/mode intact;
failure after publication is reported and retry repairs sidecar state; symlinked
targets remain untouched; concurrent policy creation preserves the winner;
uninstall preserves edited/foreign content; temporary files are cleaned on
success and error. Use deterministic fault injection rather than host-dependent
permission failures.

**Done when:** there is one replacement implementation, new policy creation never
clobbers an existing target, and failed/retried installs preserve ownership.

### D3 — Make diagnostics reflect actual state

**Source:** `doctor/adapters.rs:92` (`check_adapters`), `doctor/mod.rs:62`
(`run`), `adapter/ops.rs:375` (`status`), and `commands/check.rs:187` (`emit_v1`).

Inspect installable and cleanup-only registrations in local and global scopes.
Identify legacy registrations as unsupported and point to the existing
ownership-aware uninstall command, not an unavailable reinstall. Convert status
I/O or parse errors into report failures with their paths and remediation;
do not discard them. Distinguish one physical artifact referenced by multiple
scopes from independent installations.

Include the optional Git hook in diagnostics, including obsolete owned command
forms. Inspect owned files read-only; do not execute arbitrary installed hook
content or infer ownership from a filename alone. JSON may add explicitly
documented diagnostic rows; retain the existing report fields and mark scope
in details. Fixtures pin the local/global meanings and deduplication behavior.

Human evaluation output prints the top-level error and every `not_run` ID/reason
as well as completed results and final status. Retain stdout/stderr channel
conventions, exit meanings, and the schema-7 result structure.

**Regressions:** global-only Pi; local/global legacy Codex registrations;
registered missing artifact; malformed/unreadable settings; modified/foreign
hook; legacy Git hook; top-level policy drift after successful commands;
total-timeout with remaining checks. Assert useful human reasons and matching
JSON/exit behavior, not just the presence of a status word.

**Done when:** stale or unreadable owned installations are visible, remediation
is valid for their support status, and incomplete runs explain why they stopped.

### D4 — Share policy parsing, selection, and snapshots

**Source:** `config/v1.rs:48` (`V1Config`), `commands/config.rs:75`
(`inspect_v1_bytes`), `commands/explain.rs:64` (`v1_change_status`),
`runner/v1.rs:17` (`evaluate_v1`), and `trust/policy_hash.rs:145`
(`ApprovedPolicy`; this span postdates its graph card).

Expose a validated v1 policy with private fields and read-only accessors. Parse
each loaded snapshot once. Keep strict duplicate/unknown-field rejection and
compile file matchers once during validation. A shared selection function returns
selected IDs and explainable decisions for acceptance, unknown paths, known-empty
paths, and known matching/nonmatching paths. Use it in evaluation and `explain`.

Replace the CLI inspection structs, duplicate defaults/files deserializer, and
single-variant legacy config wrapper with the shared model. Adapt
`show-resolved-config`, `doctor`, `validate`, and schema examples to that model.
Read-only operations load/validate without consulting or writing consent.

Separate captured policy/script identity from consent-store approval. Core
evaluation takes a validated immutable snapshot, root, typed event, and optional
changed paths. The CLI obtains local approval before execution. Library callers
may supply a snapshot without creating a local consent store; this is a library
evaluation API, not repository publication authorization. The existing public
approved-policy evaluator forwards to the new implementation. Retain exact-byte
parsing and pre-check/post-run drift verification in both paths.

**Regressions:** execution and explanation select identical IDs across a table
of events/path states; all read-only commands work with no consent store and
create none; a library evaluation works in an isolated tree without consent I/O;
CLI execution still denies untrusted policy with exit 4; unchanged consent stays
valid; mutations between approval and execution still deny acceptance; existing
public callers continue to compile.

**Done when:** one policy model and selector serve all callers, and evaluation
has no dependency on consent-store lookup or installer state.

### D5 — Bound verification and execution with one deadline

**Source:** `runner/v1.rs:27` / `:41` / `:49`, `engine/execution.rs:68`, and
`trust/policy_hash.rs:80` / `:178` / `:236`.

Start the total execution-batch deadline immediately before selection/evaluation
of the validated snapshot. It covers selection, pre-check verification, commands,
pipe handling, and final verification. Initial snapshot loading and local consent
lookup remain pre-execution work; document that distinction. Carry the same
absolute monotonic deadline through those operations and check it after
verification, before spawning. Per-check deadlines cap command execution.

Verification checks expiry at traversal and read-chunk boundaries. Streaming
script content removes the in-memory copy of the entire script tree: retain
policy bytes, ordered path metadata, and digests, plus a fixed read buffer.
Preserve sorted labels, byte-length prefixes, and exact hash framing; reject
mid-read size changes, disappearing files, and unsupported entry types. Compare
ordered digest sets without quadratic searches when reporting drift.

Keep cancellation visible to pipe-drain loops even when an escaped descendant
continuously writes; observing cancellation must not depend on a `WouldBlock`.
Preserve the executor's existing output caps, stdin/environment rules, process
cleanup, and bounded grace period.

No command starts after budget expiry. Final verification that cannot complete
within budget produces `error`, even when every command passed; preserve completed
results and mark only genuinely unexecuted selected checks `not_run`. Use the
existing `total_timeout` reason for invocation exhaustion and preserve stable
execution/drift classifications.

An uninterruptible filesystem operation can overrun a cooperative deadline, and
process cleanup has a fixed grace period. Document these bounds rather than
promise hard OS I/O cancellation. Time arithmetic uses pure helpers accepting
`now`/deadline values; tests advance supplied time rather than sleep for budgets.

**Regressions:** verification consumes the last budget and no command marker is
created; expired final verification denies pass; deadline overflow; stable hash
vectors and consent entries; large script content uses bounded content buffers;
add/remove/mutate during traversal fails closed; continuous-output escaped child
cannot retain drain threads; direct child cleanup and invalid-UTF-8 output remain
covered. Keep one bounded real subprocess timing smoke test alongside pure time
tests.

**Done when:** verification cannot grant a fresh execution budget, streaming
hashes preserve consent identity, and cancellation frees drain resources.

### D6 — Add per-check timeout overrides

Add one optional positive integer `timeout_secs` directly to a check:

```yaml
version: 1
execution:
  timeout_secs: 30
  total_timeout_secs: 300
checks:
  format:
    on: [change, accept]
    timeout_secs: 10
    run: cargo fmt --all --check
  tests:
    timeout_secs: 180
    run: cargo test --locked
```

The effective command timeout is the smaller of the check override (or global
default) and remaining total budget. An override may exceed the global default;
it cannot extend the invocation deadline. Absent overrides retain v1 behavior.
Reject zero, negative, fractional, null, string, duplicate, and unknown fields;
handle representable-but-impossible deadlines through existing execution errors.
The field changes the approved policy bytes and therefore requires renewed consent.

Update spec §§6–8, the authoring skill/schema output, policy and CLI references,
writing-check guides, and inspection output using the D4 policy model. Document
minimum evaluator version and downgrade behavior. Do not make adapters parse a
second copy of the policy to discover this value.

**Regressions:** inherited default; shorter and longer override; total budget
wins; both events use the same check override; validation failures; old policy
fixtures unchanged; updated policy needs renewed consent; exact inspection values
and real mixed-duration command execution.

**Done when:** the example runs, policy tooling agrees on the timeout, and the
version/consent implications are documented without changing schema 7.

### D7 — Bound Pi feedback subprocesses

**Source:** `adapters/pi/src/index.ts:76` (`runIronLint`) and `:144`
(`ironlintExtension`); tests in `adapters/pi/test/index.test.ts`.

Give each invocation an owned run handle with cancellation, bounded byte capture,
and one settlement path. Adapter safety ceilings are 600 seconds wall time,
8 MiB stdout, 64 KiB retained stderr, and a 2-second termination/closure grace.
These are host protection limits, separate from policy command budgets. A valid
policy exceeding a feedback ceiling receives an explicit incomplete-feedback
diagnostic and standalone reproduction command; it is never presented as a pass.

Stop and clean up on wall-time expiry or stdout overflow. Clip/drain excess
stderr and identify truncation. Never parse truncated stdout as a complete JSON
document. Count bytes before UTF-8 decoding; split multibyte characters and JSON's
byte-array expansion must not circumvent the cap or corrupt valid bounded output.

A newer successful mutation cancels an older active run, including when the new
mutation removes the policy. Associate cancellation with that run's identity so
late events cannot cancel or settle its replacement. Actively canceled callbacks
do not append stale failure output; preserve a superseded indication for already
completed results that race a newer edit. Keep advisory post-edit feedback and
the original edited content.

Use supported process-group cleanup where available. A missing `close` callback
cannot leave the promise pending forever: watchdog/cancellation owns a bounded
completion path, destroys capture streams, clears timers/listeners, and reports
cleanup failure when direct-child closure was not observed. Do not claim full
process isolation or Windows descendant guarantees beyond the supported scope.

Independent review reproduced a running check surviving evaluator termination:
the core intentionally gives each check its own process group. Add an opt-in
stdin-close cancellation channel at the CLI boundary and an explicit core
cancellation token. Pi requests cooperative cleanup first; the evaluator kills
and reaps its active check using the existing command cleanup, without starting
another check. Retain the bounded forced-stop fallback. Checks still receive
closed stdin, and existing evaluator entry points retain their behavior. Test
this interaction with the real CLI, not only a same-group fake evaluator.

**Regressions:** hung evaluator; sustained noisy stdout/stderr; split UTF-8
chunks; oversized otherwise valid JSON; missing binary; malformed JSON; child
error/close/timeout races; close callback absent; cancellation during output;
older failure followed by newer success; policy deletion; no surviving direct
child, ordinary check group, or timer after ordinary cancellation. Preserve the
provenance fixtures.

If Pi never delivers a `tool_result`, feedback cannot be produced: document that
boundary and retain full acceptance independently. Fixture tests prove callback
handling, not real host event delivery. Run `adapter-drift-audit pi` only if the
harness contract itself changes; this packet changes subprocess handling only.

**Done when:** subprocess/output lifetimes are bounded, cancellation is scoped
correctly, and an incomplete feedback run cannot look successful.

### D8 — Simplify interactive setup

**Source:** `init/select.rs:43` (`prompt_multi_select`),
`init/onboard.rs:223` (`confirm_gate_to`), and the CLI manifest's `ratatui` dependency.

Replace raw-terminal multiselect with an ordinary line-based confirmation over
the printed plan. A detected Pi adapter is the default selection; an undetected
Pi installation requires an affirmative choice. Explicit `--harness`, `--yes`,
noninteractive behavior, `--dry-run`, and optional `--git-hook` retain their
documented authorization meanings. Interactive uninstall lists owned Pi
registrations in the requested scope and owned legacy registrations in both
local/global scopes, then confirms their cleanup; explicit `--harness all`
continues to select legacy cleanup.

Remove the terminal renderer/reducer/guard and dependencies with no retained
callers. Regenerate Cargo.lock and preserve the MSRV. Retain equivalent tests
for choices, cancellation, dry-run isolation, and installation outcomes; replace
obsolete rendering snapshots with behavior coverage. Normal stdout/stdin is
enough for this stable confirmation flow, so a terminal UI framework is no longer
needed. Measure dependency count and release binary size before/after using the
same toolchain/target; record results without imposing an invented size gate.

**Done when:** setup and cleanup work through a plain terminal or explicit flags,
terminal state is never changed, and the unused TUI dependency tree is removed.

### D9 — Integrated verification and documentation

Update implemented architecture, relevant spec/reference sections, adapter and
setup documentation, authoring instructions, and this resume block to match the
actual result. Keep the v1 release evidence as its baseline; record fresh evidence
for this plan against the integrated SHA or clearly named working-tree scope.
Refresh the graph after the code changes and remove obsolete generated context
for removed symbols. Apply `cleanup-build-artifacts` to task-created scratch and
release artifacts, preserving the normal iterative `target/` and user files.

Run required gates sequentially per Cargo target directory:

```sh
rtk cargo test --locked
rtk cargo clippy --locked --all-targets -- -D warnings
rtk cargo fmt --all --check
rtk proxy bash scripts/ci-coverage.sh
rtk proxy bash scripts/ci-adapters.sh
rtk proxy bash scripts/test-run-containerized-acceptance.sh
rtk proxy bash scripts/test-verify-acceptance.sh
rtk proxy bash tests/e2e/features/run.sh
rtk proxy bash tests/e2e/init/run.sh
```

Supply a temporary XDG_CONFIG_HOME to the adapter lane. Run the repository's
MSRV/Windows compile gates and the Pi type-check/tests through their existing CI
lanes. Rust files must retain at least 80% region coverage and cognitive
complexity at most 15. Record observed counts and versions; do not substitute
the earlier 397-test/92.01%-coverage release numbers.

Extend Docker features only where they exercise a changed consumer boundary:
complete acceptance after mixed per-check budgets and truthful incomplete
results. Installer network failures use local fakes; live harness and hosted
enforcement qualification remain adapter/integration-owned.

**Done when:** all packets and scoped agent reviews are closed, required checks
pass on the integrated tree, docs/code agree, and task-created artifacts are
removed. Publication is a separate operator action.

## Decision Inventory

This pressure test applies to the plan above; it is part of this document rather
than another execution plan.

| # | Decision | Explicit or implied? | Where |
| --- | --- | --- | --- |
| 1 | Retain the two crates, serial evaluator, and schema-7 result shape | Explicit | Scope |
| 2 | Preserve consent-store shape and exact hash framing | Explicit | Scope, D2, D5 |
| 3 | Add an optional check-level timeout to version: 1 | Explicit | Scope, D6 |
| 4 | Keep the old public evaluator as a forwarding wrapper | Explicit | Scope, D4 |
| 5 | Use one validated model and compiled selector | Explicit | D4 |
| 6 | Put consent approval at the CLI boundary | Explicit | Scope, D4 |
| 7 | Use exclusive new-file creation and atomic replacement | Explicit | D2 |
| 8 | Serialize owned read-modify-write resource groups | Explicit | D2 |
| 9 | Preserve ownership and repair incomplete installs on retry | Explicit | D2 |
| 10 | Fetch the installer before synchronous execution | Explicit | D1 |
| 11 | Bound updater download/total time; use no automatic retries | Explicit | D1 |
| 12 | Inspect both installation scopes and surface diagnostic errors | Explicit | D3 |
| 13 | Carry an absolute execution-batch deadline | Explicit | D5 |
| 14 | Stream script content, retain metadata/digests and policy bytes | Explicit | D5 |
| 15 | Bound Pi output/time independently of policy budgets | Explicit | D7 |
| 16 | Cancel an obsolete run by its identity and settle once | Explicit | D7 |
| 17 | Use a plain confirmation and remove the TUI dependency | Explicit | D8 |
| 18 | Require isolated regression evidence and scoped reviews | Explicit | D0, D9 |
| 19 | Keep policy defaults separate from adapter safety ceilings | Explicit | Scope, D6, D7 |
| 20 | Add diagnostic rows while retaining the doctor report fields | Explicit | D3 |

## Irreversibility Triage

| # | Classification | Isolation strategy |
| --- | --- | --- |
| 1 | ONE-WAY | Retain schema 7 and existing meanings; any future incompatible result requires a new schema. |
| 2 | ONE-WAY | No stored-shape/framing migration; pinned hashes and old consent fixtures gate changes. |
| 3 | ONE-WAY | Optional field behind the versioned parser; old configs unchanged, old binaries reject the field, removing it permits downgrade; document minimum release. |
| 4 | ONE-WAY | Additive API and retained forwarding wrapper isolate existing callers; future removal requires separately scoped breaking release. |
| 5 | TWO-WAY | n/a — private representation with read-only accessors. |
| 6 | TWO-WAY | n/a — internal separation while retaining the CLI approval gate and public wrapper. |
| 7 | TWO-WAY | n/a — shared helper, no new persistent shape. |
| 8 | TWO-WAY | n/a — operation locking with stable acquisition order. |
| 9 | TWO-WAY | n/a — retry behavior constrained by existing ownership records. |
| 10 | TWO-WAY | n/a — same receipt/channel, independent subprocess outcomes. |
| 11 | TWO-WAY | n/a — internal constants and explicit failure. |
| 12 | TWO-WAY | n/a — read-only inspection and existing cleanup commands. |
| 13 | TWO-WAY | n/a — stable error reasons and documented cooperative bounds. |
| 14 | TWO-WAY | n/a — unchanged hash output, bounded content buffers. |
| 15 | TWO-WAY | n/a — advisory feedback reports a ceiling with CLI reproduction. |
| 16 | TWO-WAY | n/a — invocation-local lifecycle state. |
| 17 | TWO-WAY | n/a — explicit flags remain and choice defaults are specified. |
| 18 | TWO-WAY | n/a — implementation evidence/review procedure. |
| 19 | TWO-WAY | n/a — distinct meanings prevent duplicated-policy drift. |
| 20 | ONE-WAY | Add rows without removing/changing fields; document scope in details and pin JSON fixtures before adoption. |

## Fork Audit

### Normalize vs denormalize

- Inventory rows: #5, #19.
- Plan chose: one policy model; separate adapter ceilings with distinct meanings.
- Chosen column quoted: "the duplicated fact feeds a correctness decision" —
  met YES: D4 makes selection/validation authoritative across execution and inspection.
- Opposing column quoted: "the read path is measured-hot" / "staleness is
  tolerable with a bound" — met NO: this plan supplies no measured need for a
  stale duplicate policy model or budget.
- Satisfied: YES.

### Sync call vs queue/background job

- Inventory row: #10.
- Plan chose: a directly observed synchronous update.
- Chosen column quoted: "The caller must abort if the work fails" — met YES:
  D1 executes no partial installer and reports update failure.
- Opposing column quoted: "the work calls an external service whose
  latency/availability you don't control" — met YES for the updater download.
  D1 rejects the alternative explicitly: "queued installation would add persisted
  jobs and change return semantics."
- Satisfied: YES; that semantic requirement justifies the synchronous choice.

### Build vs adopt a dependency

- Inventory rows: #7, #17.
- Plan chose: a small shared filesystem helper using existing primitives, and a
  plain confirmation flow.
- Chosen column quoted: "The need is under ~200 lines and the requirements are
  stable enough to write down today" — met YES for the replacement helper and
  confirmation: D2/D8 enumerate their fixed operations; reevaluate scope if the
  helper needs a transaction framework.
- Opposing column quoted: "The problem is distant from your core domain" AND
  "the candidate shows checkable maintenance signals" — met NO: no new candidate
  dependency is proposed or evaluated; D8 explains why the terminal framework
  is unnecessary for the reduced flow.
- Satisfied: YES.

### Optimistic vs pessimistic concurrency

- Inventory rows: #8, #9.
- Plan chose: serialize IronLint mutations of shared resource groups.
- Chosen column quoted: "Acting on stale data is expensive in the real world"
  — met YES: D2's read-modify-write operations must preserve user hook/settings
  content and bind artifact ownership consistently.
- Opposing column quoted: "Conflicts are rare" AND "retry is cheap" — met NO:
  concurrent agent sessions are an explicit regression and an overwritten
  user settings change is not reconstructed by rerunning installation.
- Satisfied: YES.

### Config vs code

- Inventory rows: #3, #11, #15, #19.
- Plan chose: reviewed policy command budgets in config, lifecycle ceilings in code.
- Config column quoted: "An operator must change the value without a deploy"
  — met YES for D6 command budgets; policy edits/consent and removal of the
  optional field give the audit/downgrade path.
- Code column quoted: "The value is an invariant other code silently assumes"
  — met YES for D1/D7 capture/cleanup ceilings; their tests and lifecycle logic
  share the constants, and they do not define policy acceptance requirements.
- Satisfied: YES; the columns apply to different values with explicit meanings.

### Migration strategy

- Inventory rows: #2, #3, #4, #20.
- Plan chose: retain stored consent/hash formats and add optional config/API/report
  capabilities while retaining existing consumers' shapes and entry points.
- Chosen column quoted: "Real data exists in production" — met YES: existing
  consent entries, policies, and API consumers are explicitly retained.
- Opposing column quoted: "the table can be dropped and rebuilt without anyone
  noticing" — met NO: the plan requires unchanged consent and caller compatibility.
- Satisfied: YES; this additive change needs no destructive data backfill.

No fork match: #1, #6, #12, #13, #14, #16, #18; these are retained contracts,
module boundaries, local diagnostics, deadline/content algorithms, invocation-local
process lifecycle state, and validation procedures rather than distributed
service/storage/deletion/event-stream forks.

## Corner Scan

- Unmigratable schema: ABSENT — Scope/D6 retain old policies and hashes, isolate the optional field through the parser, and specify downgrade/minimum-version behavior.
- Side effects without idempotency keys: ABSENT — D2 preserves ownership and checks published artifacts plus sidecars on retry, while D1 explicitly performs no automatic installer retry.
- Test-hostile boundaries: ABSENT — D1/D2 use local executable/fault seams, D4 exposes policy decisions independently, and D5 accepts supplied time in pure deadline helpers.
- Auth/tenancy bolted on later: ABSENT — Scope/D4 retain the CLI consent gate and distinguish library evaluation from an external owner's publication authority.
- Unbounded growth: ABSENT — D1/D2 clean scratch files and D7 bounds capture/run state without adding persistent history.
- Hidden fan-out: ABSENT — D5 operates locally on streamed files with deadline checks and retains serial command dispatch, introducing no per-item network calls.
- Shared mutable state across workers: ABSENT — D2 serializes shared resource mutations and D7 confines generation/cancellation state to one extension instance.
- Clock in the logic: ABSENT — D5 specifies monotonic deadlines with supplied now/deadline inputs for decision tests and D1/D7 clear owned timers.
- Hard external coupling, no failure mode: ABSENT — D1 caps download/update time and fails explicitly without automatic retries, and D7 handles hung/missing evaluator closure with bounded completion.
- "Pagination later": ABSENT — no list endpoint is introduced; D5's script-content streaming and D7's explicit output ceiling address their actual local resource boundaries.

## Pre-Mortem

1. **A retry leaves the adapter unowned.** Mechanism: a plugin write succeeds,
   sidecar publication fails, and a byte-equality early return skips repair.
   First symptom: uninstall refuses an apparently installed adapter. D2's retry
   rule and post-publication fault regression specifically prevent this path.
2. **Valid feedback trips the capture ceiling unexpectedly.** Mechanism: schema-7
   byte arrays expand raw diagnostics in JSON, and multibyte text spans chunks.
   First symptom: a chat shows incomplete feedback or corrupted diagnostics.
   D7 explicitly counts transport bytes, tests large valid JSON/split UTF-8,
   and reports a ceiling with reproduction instead of claiming pass.
3. **The expected external event never arrives.** Mechanism: Pi emits no
   `tool_result`, or a child never emits `close`; first symptom: feedback is absent
   or a pending invocation accumulates. D7 documents missing host events and
   retains independent acceptance, while its watchdog owns settlement even when
   child closure is absent. Real host delivery remains adapter-owned qualification.

## Verdict

**VERDICT: PASS** — the saved plan incorporates the recovery and compatibility
rules exposed by the pressure test. This verdict concerns the design, not runtime
test status or permission to publish.

Findings that survived the discard rule: NONE.

DISCARDED: byte-identical retry skipping ownership repair — D2 explicitly requires
metadata inspection/repair and its fault regression, so the concrete failure path
is addressed in the plan.

DISCARDED: JSON expansion/chunk decoding defeating the capture limit — D7 counts
bytes before decoding, tests those inputs, and defines truthful overflow feedback.

DISCARDED: missing external callbacks stranding work or implying acceptance — D7
defines bounded settlement, documents host-event limits, and retains independent
acceptance; a missing host event is not claimed as solved by fixture tests.

Required edits: NONE; the protections above are already included in the packets.

## Evidence ledger

| Packet/batch | Commit or tree | Failing-first evidence | Focused validation | Separate review | Result |
| --- | --- | --- | --- | --- | --- |
| D0 | `e23b18c` plus planning/user hook changes | Failed downloader and partial download regressions return false success on unchanged updater | `cargo test --locked`: 397 passed, 24 suites (Rust 1.96.1) | n/a | Complete |
| A: D1–D3 | `c677ce0` | Masked shell failures; file collision/mode/ownership/symlink defects; all 13 diagnostic cases; partial first-backup publication; writable updater handle and inactive Git-hook regressions | Integrated: 463 tests / 25 suites. Focused fixes: 22 updater unit, 18 diagnostic/update integration, 12 filesystem, 14 Git unit, 8 Git integration. All-target Clippy and format clean | Separate agent found updater write-access and inactive-hook defects; both fixed and rechecked closed | Complete |
| B: D4–D6 | `33f417a` | D4: supplied bytes A incorrectly approved live bytes B through inherited worktree consent. D5: expired verification still launches a command; expired final verification reports pass; continuous drain ignores cancellation; empty selection bypasses expiry. D6: unsupported field/inspection; shorter/longer overrides ignored by global-only runner | Integrated: 504 Rust tests / 28 suites (15.81s), then all-target Clippy and format clean, strictly serial. Core timeout 8, runner 9, focused CLI 175 / 6 suites. Shipped skill validation passes; schema example parses | Independent PASS; empty-selection finding fixed and rechecked, no open findings | Complete |
| C: D7–D8 | `fb76308` | D7: overflow/hang leaves child running, stderr unbounded, newer mutation does not cancel, malformed exit-zero output suppressed; real CLI check survives evaluator cancellation; invalid UTF-8 repaired into false success. D8: PTY EOF authorizes installation; automatic cleanup treats an unowned harness directory as installed | Integrated: 516 Rust tests / 30 suites (16.36s), 37 Pi tests, typecheck, strict Clippy/fmt. Focused setup: 44 unit, 11 onboarding, 7 PTY, 10 scaffold, 3 dry-run, 8 Git. Controlled Rust 1.96.1/aarch64 release: 2,842,608 → 2,518,432 bytes (11.4% smaller); external normal dependencies 103 → 59; locked packages 266 → 145, no added or changed versions. Pi/skill pinned to B in both builds | Independent PASS; real cancellation and strict UTF-8 findings fixed and rechecked, setup 29 independent tests; no open findings | Complete |
| D9 | `fb76308` + test/harness/documentation changes in this final commit | Onboarding image lacks Git required by new optional-hook inspection; instrumented updater success test exceeds short fake-download deadline | Final native: 516 tests / 30 suites (21.34s), strict Clippy (1.41s), fmt. Coverage: 93.93% regions, all files ≥80%. Isolated adapter script: 37 tests; typecheck passes. Rust 1.88 build and 516 tests pass. Windows production workspace check passes. Both Docker suites and both acceptance-wrapper test scripts pass | Harness/runtime dependency fix and test-only completion-budget correction independently PASS; no open findings | Complete |

### Final verification scope and cleanup

- Runtime implementation is recorded in `c677ce0`, `33f417a`, and `fb76308`.
  Final verification includes the subsequent test-only updater timing correction,
  locked CI/coverage builds, mixed-timeout Docker scenario, and onboarding image
  dependency correction. Native gates ran serially against the repository target;
  Docker builds used separate container filesystems.
- Rust/Cargo 1.96.1 on `aarch64-apple-darwin` ran the final native gates.
  Rust 1.88 ran the build and complete test suite; both Docker images also built
  on Rust 1.88. The `x86_64-pc-windows-msvc` check is compilation evidence only
  (one existing Unix-only signal-variant dead-code warning). No native Windows
  runtime or live Pi event-delivery/enforcement qualification is claimed.
- `graft build` refreshed 93 source cards, 1,319 nodes, and 1,616 edges after
  removing 21 generated cards with stale provenance and no manual notes. The
  graph remains an ignored local cache; no model-backed graph rebuild ran.
- Applied `cleanup-build-artifacts`: removed the controlled measurement checkout,
  task coverage output, two task-created onboarding run directories, and task
  Docker images. Preserved the normal debug/adapter cache, pre-existing release
  binary (SHA-256 `28255a8ed10e3eb9e19246b48c6b3afb921fb31357d45e07c5e78649553a1f06`),
  prior run artifacts, and the user's hook removal/backup.
- The first local commit triggered the existing Repowise post-commit hook, whose
  log showed source snippets sent to its configured model service. The owned
  updater process was stopped; subsequent commits used a one-command hook-path
  override. The user's hook configuration and unrelated processes were preserved.
