# Close the Claude Code and Codex completion gap

Status: **N0–N3 complete for the explicit controlled entry points and exact runtime/platform scope recorded below. Unpublished.**
Date: 2026-10-02.
Baseline: `main` at `75bc96611373b975403140145f0477cae7a75ffb`.

## Implemented outcome

The maintainer wants deterministic completion, not another compatibility layer.
Close item 1 from the release-readiness discussion: Claude Code and Codex must
not authorize successful completion after a failed or incomplete acceptance run.
Keep repair possible, but terminate exhausted repair as incomplete rather than
success. ACP was considered and explicitly rejected; do not reopen it.

This is the focused follow-up to the native packaging work in the
[coding-harness compliance plan](2026-10-01-harness-compliance.md). That plan
retains Pi's implementation and qualification evidence. This document owns the
new native completion work; it does not repeat or reopen its completed packets.
Live repair utility measurement and publishing remain separate release items.

The implemented routes are
[`ironlint-claude-complete`](../adapters/claude-code/completion.md) and
[`ironlint-codex-complete`](../adapters/codex/completion.md). Ordinary native
sessions retain their feedback hooks; they do not acquire a completion claim.
Both controllers withhold model text and successful terminal status until a
fresh complete acceptance pass for the returned captured artifact. Repair,
cancellation, errors, and exhaustion terminate within the declared bounds.

## Current behavior and the concrete defect

Both native adapters share `adapters/shared/hooks/hook.py`. `feedback()` at
line 127 returns `decision: block` for an acceptance violation only when
`stop_hook_active` is not true. Continued violations return only `systemMessage`.
Execution/incomplete errors also take that warning-only branch. A warning does
not deny the host's normal completion operation.

The tests deliberately pin this behavior: `test_hooks.py` lines 50 and 132
expect continued failures to contain an incomplete warning without another
blocking decision. Missing binaries, consent failures, and background work are
likewise tested as warnings. Change those expectations through new failing
regressions; do not describe the existing suite as proof of enforced completion.

Acceptance already runs fresh, checks the complete required ID set, validates
schema 7 and exit/outcome agreement, rejects policy drift, and uses bounded
subprocess execution. Preserve those checks. The missing piece is what the
harness is allowed to do after denial, failure, or exhausted repair.

Native hooks evaluate the mutable workspace. Pi's separate controlled runner
owns a settled task result and a captured candidate artifact. Changing a Stop
response alone does not establish Pi's candidate guarantee.

## Required outcome and scope

For each supported entry point, identify the exact operation called completion
and the host-owned channel that records success or incomplete work. The invariant
is: **successful completion requires a fresh, complete acceptance pass for the
candidate being completed; every other outcome withholds success.**

- Repeated violations remain denied, including an already continued Stop.
- Errors, missing prerequisites/consent, malformed or truncated output, unrun or
  mismatched checks, cancellation, timeout, and uncertain settlement never pass.
- Cancellation may end the session as cancelled/incomplete; it need not leave
  the agent permanently running or require passing checks to exit.
- A bounded repair budget ends with an explicit non-success result. A display
  warning or the model saying “done” is not that result.
- Post-edit checks remain feedback: red TDD and intermediate refactors stay
  editable. Full acceptance does not depend on receiving every edit event.
- Policy, expected IDs, evaluator choice, and completion authority remain
  owner-selected. Never grant trust, weaken policy, or execute repair commands
  supplied in diagnostic text.

Target Claude Code and Codex on the currently documented Linux/macOS adapter
platforms. Preserve Pi, the stable CLI/schema contract, installer ownership and
user edits. Share generic acceptance/decision logic; keep host response mappings
separate wherever the contracts differ.

Do not add new harnesses, ACP, a daemon, scheduler, database, general permissions
framework, policy language, or OS sandbox. Do not introduce a force-pass switch
or reusable pass cache. Distribution overhaul, marketplace publication, broad
model benchmarks, and Windows qualification are outside this packet.

Distinguish local workflow completion from the stronger six-part enforcement
contract in [spec section 4](../specs/2026-09-05-ironlint-v1-design.md#4-enforcement-contract-and-authority).
Same-account hooks do not create independent authority or tamper resistance.
Candidate binding must be proved for any claim of parity with Pi. Reuse the
existing candidate contract if a controlled path is necessary; do not invent a
second definition of candidate or imply that before/after hashes prevent all
concurrent write-and-restore attacks.

## N0 — Prove the host contract before choosing the implementation

- [x] Run the repository's `adapter-drift-audit` for each harness and fetch
  current primary documentation through Context7. Inspect the exact installed
  or pinned runtime's hook dispatcher, not just names shared between manifests.
- [x] Record the runtime version, platform, primary source revision/reference,
  observed payloads, and actual host outcome for first/continued Stop, hook
  error/timeout, cancellation, missing/disabled hooks, and conflicting hooks.
- [x] Establish what `decision: block`, process exit codes, `systemMessage`,
  and `stop_hook_active` actually do in each host. An observed install version
  is not a qualified version range.
- [x] Prove how automatic repairs can stop within a limit while the host records
  incomplete work. Test a model that ignores repair guidance. Inspect retries,
  queued messages, nested sessions, and background/detached mutations.
- [x] Establish candidate settlement and identity: what is evaluated, what can
  still write, and what bytes/result the completion operation accepts.

At baseline, the observed CLIs were Claude Code 2.1.207 and Codex 0.159.1; their
live completion behavior is unqualified. These are starting observations, not
required support pins. Use [Claude's hooks reference](https://code.claude.com/docs/en/hooks)
and the current official Codex hook/runtime sources located by the audit; do
not assume matching event names mean matching authority.

Write a short capability record here for each harness, with one conclusion:

1. **Native hooks suffice:** prove denial, bounded terminal incomplete behavior,
   settlement, candidate binding, and the declared supported routes.
2. **A controlled entry point is needed:** document the missing host capability
   and propose the smallest adapter-owned SDK/CLI controller that owns the
   terminal result, with invocation, candidate, repair, and cancellation behavior.
3. **Unsupported:** state the concrete limit and withhold the completion claim.

Prefer native hooks when they satisfy the invariant. Reject blindly returning
`block` forever: it may cause infinite repairs and still cannot fix a host that
ignores errors, suppresses Stop, or permits writes after acceptance. Use a
controlled entry point only when N0 demonstrates that need. Do not silently
replace ordinary native sessions or implement an unreviewed new launch contract;
write the concrete fallback design here before proceeding to N1/N2.

N0 may finish with a capability limitation. That is useful evidence, not a
completed enforcement implementation. Keep affected completion packets unchecked
until an actual supported entry point meets the invariant.

### N0 capability records and selected boundary

| Harness | Exact runtime and observed native limit | Supported controlled conclusion |
| --- | --- | --- |
| Claude Code | 2.1.207; persistent blocks reach nine Stop callbacks and then headless success; warning/error/timeout/disabled hooks can also finish successfully | Controlled entry point required. Fresh bare Read/Edit sessions, strict `terminal_reason: completed`, owned process settlement, and captured acceptance qualify `ironlint-claude-complete`. |
| Codex | 0.159.1; continued warning, hook errors/timeouts, and competing `continue:false` record `turn.completed`; repeated blocks have no owned repair limit | Controlled entry point required. Fresh restricted exec, exact `gpt-5.5` direct tool mode, strict JSONL settlement, and captured acceptance qualify `ironlint-codex-complete`. |

Primary documentation, runtime hashes/source revisions, actual payloads, audit
findings, and replay commands are in the
[Claude evidence](../adapters/claude-code/completion-evidence.md) and
[Codex evidence](../adapters/codex/completion-evidence.md). The Codex audit skill
lacks a historical reference/watermark; its record explicitly distinguishes
that unavailable history from the current primary-source/runtime audit. No
watermarks were silently changed.

Both exact pins were exercised on macOS arm64 and Debian Bookworm Linux arm64
with real IronLint and localhost deterministic model transports. Claude's Linux
replay used the standard cached container. Codex's Linux sandbox required the
outer Docker syscall filter to be unconfined; no privileged mode or added
capabilities were used, and Codex's own sandbox stayed enabled. The default
Docker filter failed closed on patches and is outside the qualified repair
route. Other runtime versions, architectures, distributions, model/tool modes,
ordinary native/interactive/resume routes, and live model utility are unqualified.

## N1 — Add failing regressions for the chosen boundary

### Selected fallback design (2026-10-02 implementation session)

The native Stop route cannot supply all required capabilities: the current
Claude reference caps repeated continuations, hook infrastructure failures do
not establish terminal incomplete work, and Codex exposes competing-hook
completion overrides. Neither route returns a captured, accepted artifact.
The per-runtime evidence below will distinguish documentation/source findings
from observed dispatch and deterministic real-runtime replay.

Implement **explicit** `ironlint-claude-complete` and `ironlint-codex-complete`
launchers. They are adapter-owned local controllers, invoked separately from
ordinary native sessions; installation does not add another Stop registration.
Each controller owns its single JSON terminal result and process exit: only
`complete`/exit 0 includes held model text and the freshly accepted candidate;
everything else is `incomplete`/exit 3. Model assertions and native hook messages
cannot set that result. Unknown runtime versions or unproved settlement routes
fail incomplete instead of inheriting a completion claim.

Use shared Node.js TypeScript orchestration with no model SDK dependency and
host-specific CLI arguments/result parsers. Reuse the **existing Pi**
`candidate.ts` capture/identity implementation and `acceptance.ts` strict
schema-7 classifier as canonical package inputs; do not port the candidate
definition. The owner supplies absolute external policy, evaluator, artifacts,
runtime/configuration, and exact required IDs. Runtime configuration is isolated
from candidate-owned hooks/plugins wherever the pinned host supports that;
any remaining unsupported tool/background route must return incomplete.

Every model invocation is owned, synchronous, bounded, and settled before
capture. Missing/error/ambiguous terminal events cannot authorize capture.
After capture, run every acceptance check against the artifact tree and compare
artifact and source manifests. Persistent artifact edits invalidate acceptance;
source drift gets at most two recaptures within the same budget. Violations
permit at most three repair invocations by default, with diagnostics treated as
untrusted explanatory text. Infrastructure/consent errors stop immediately.
Retries start a new process attempt and fresh evaluation, without pass caches,
resume authorization, or persistent repair counters.

One injectable monotonic deadline covers runtime verification, model turns,
capture, evaluation, and two-second subprocess cleanup, with a default total of
900 seconds. Preserve native hook bounds separately. Serialize the same
canonical Git workspace across artifact stores using an exclusive attempt lock;
contention or an uncertain stale lock ends incomplete, with no automatic
takeover. Neither capture nor locking stages/commits or changes the branch.
Discard abandoned capture artifacts; successful artifact ownership transfers
explicitly to the caller. Cancellation stops owned process groups and returns
incomplete; uncertain cleanup remains a diagnostic and never success.

Qualification will use the actual pinned native CLIs and deterministic local
model transports, with isolated HOME/XDG/harness configuration and dummy local
credentials. Simulated controller sessions prove internal contracts only.
Missing real-host evidence keeps the affected N3 qualification unchecked.
This is local workflow completion under the same-account limits already stated
for Pi, with no publication authority or stronger independent enforcement claim.

- [x] Preserve existing schema/process/packaging coverage and add regressions
  that fail on the baseline warning-only Stop behavior.
- [x] Test the pure acceptance-to-completion decision separately from host I/O;
  then prove the host consumes that decision as intended.

| Scenario | Required host-owned result |
| --- | --- |
| First violation and repeated continued violations | No successful completion |
| Repair succeeds, then another edit introduces a violation | Fresh denial; no stale pass |
| Missing binary/policy/consent, execution error or timeout | Incomplete/denied, with actionable diagnostics |
| Wrong event/schema/IDs, truncated or malformed output, `not_run` | Incomplete/denied |
| Shell write, delete/rename, or no matching change check | Every required accept check still runs |
| Background/queued/late mutation or source drift | Settle and re-evaluate within bounds, otherwise incomplete |
| Check changes evaluated source and exits zero | No acceptance of the altered candidate |
| Repair limit exhausted or model ignores guidance | Bounded terminal incomplete; no success and no endless loop |
| User cancels or host interrupts a hook/controller | Cancelled/incomplete; owned subprocesses settle |
| Disabled/missing Stop, hook crash, competing hook or duplicate registration | No success in a claimed protected route; otherwise classify that route unsupported |
| Resume/restart or a different project/session | Fresh acceptance; no inherited pass or repair authorization |

“The hook printed JSON” is not a host outcome. Deterministic replay through the
real runtime should establish delivery and denial where possible. Keep any live
probe focused on this boundary; broad repair utility work remains in item 2.

## N2 — Implement the smallest demonstrated fix

- [x] Trace direct callers before editing. Keep result classification and the
  decision function testable with plain inputs. Preserve complete check-set
  validation; exit zero alone never authorizes completion.
- [x] Apply harness-specific response mapping only after N0 proves its behavior.
  If a controlled path is necessary, hold its terminal success until settlement,
  fresh capture/evaluation, and candidate identity checks pass.
- [x] Keep evaluation synchronous with the completion decision. Background
  checks lose the boundary because the host could finish before their result.
- [x] Use no persistent approval/repair store. If the chosen runtime needs repair
  state across callbacks, specify its owner and stable project/session/attempt
  identity before implementation; a stateless spawned hook cannot keep a useful
  process-local counter. Missing or corrupt state must not authorize success.
- [x] Serialize attempts for the same controlled candidate and define concurrent
  invocation behavior. Never share a global counter between projects/sessions.
- [x] Preserve current subprocess limits: 600-second hook budget, 610-second
  native timeout, 8 MiB input/stdout cap, 64 KiB retained stderr, and two-second
  cleanup grace. Do not silently multiply those into an unbounded repair loop.
  A controller may reuse Pi's three-repair/900-second defaults only after its
  total budget includes model work, capture, checks, and cleanup. Use one
  injectable monotonic clock/deadline source for new budget logic.
- [x] Terminate automatic repair on infrastructure/consent errors. A fresh user
  retry begins a new evaluation; never downgrade an error to a violation or pass.
- [x] Preserve existing install/uninstall ownership. Do not register a second
  controller/Stop path silently. Any capture must leave the user's index and
  branch untouched; abandoned candidate artifacts are removed, while a completed
  artifact has explicit caller ownership.

## N3 — Review and record the supported claim

- [x] Request a separate agent review of the integrated implementation and fix
  concrete regressions. Refresh Graft after source changes.
- [x] Run the native contract/package suite with a real IronLint binary, then
  the isolated adapter lane to protect Pi. Run Rust/core checks only if their
  code or shared contracts changed; retain per-file coverage/complexity rules.
- [x] Record pinned real-host evidence for each declared completion entry point,
  including violation, repair, persistent failure, error, cancellation, and
  mutation during finalization. Missing credentials/runtime access leaves that
  harness's qualification pending; simulated callbacks cannot fill the gap.
- [x] Update current architecture, native READMEs, capability evidence, and the
  parent plan to distinguish ordinary feedback from the qualified completion
  path. Version changed adapters appropriately when preparing new packages.

Done means zero violating/incomplete candidates are recorded as successful,
clean cases complete, and denial/cancellation/exhaustion terminate within the
documented bounds. Name the exact supported entry point and runtime/platform.
Do not claim every native route is protected if only an explicit controller is.
Do not mark this packet done by changing documentation to call the old behavior
feedback-only. Publication remains a separately authorized operator action.

### N3 verification and review record — 2026-10-02

- New controller/package/CLI regressions failed before implementation. The old
  native warning behavior remains an explicitly unsupported completion route;
  actual pinned dispatcher replay proved why a new owned boundary was needed.
- The isolated adapter lane passed 17 Python contract/package tests, 25 native
  Node tests, all 63 Pi tests, and the acceptance/recipe scripts. After the final
  Codex metadata/UTF-8 fixes, focused validation passed all 17 Python tests and
  27 native Node tests. Seven opt-in runtime tests skipped in that ordinary lane
  were qualified separately through the actual pinned runtime replays below.
- Claude's 24 replay scenarios passed on both qualified platforms, including
  actual launcher single-JSON/exit-0 and exhausted single-JSON/exit-3 results.
  Codex's six real-runtime groups passed on both qualified platforms; its added
  actual launcher group passed separately on macOS and Linux. These cover clean,
  repair, persistent failure, errors, cancellation, evaluated/source mutation,
  excluded tool routes, missing/competing native hooks, and fresh invocation.
- Strict TypeScript checks passed for every shared native source and test.
  The evaluator was built with `--locked` on both platforms. Rust/core source
  and the v1/schema-7 contract were unchanged; broader core checks were not
  rerun for this adapter-only implementation.
- Separate subagents reviewed the integrated hosts/CLI/packages and shared
  controller. Findings were fixed with focused regressions: descendant group
  settlement before lock release, ambient-config inspection errors, bounded
  strict UTF-8 owner input, and standalone documentation closure. Final targeted
  reviews found no remaining concrete issue in those transitions.
- Native adapters/plugin manifests are now 0.2.0. Canonical Pi candidate and
  acceptance code remain unchanged and are exact package inputs. Installation
  ownership, native hook registrations, index/branch state, and the pre-existing
  `.codex/hooks.json` edit were preserved. Catalog pins remain unchanged; no
  tags, uploads, or publication occurred. Graft is refreshed and task-owned
  scratch is removed; normal iterative `target/` is retained.

Successful artifacts transfer to the caller. If the hard cleanup deadline cannot
confirm process-group settlement, completion remains incomplete and the attempt
lock/artifact stay retained until confirmed settlement or explicit owner recovery.
That conservative failure behavior carries no approval or repair authorization.
The claim is local workflow completion for the returned artifact under Pi's
same-account limits, not the independent six-part enforcement contract.

## Source map and session precautions

| Read first | Why |
| --- | --- |
| `adapters/shared/hooks/hook.py:76`, `:127`, `:138` | Verdict validation, warning-only Stop mapping, fresh evaluation |
| `adapters/shared/hooks/process.py:13`, `:28` | Owned process cleanup, deadline and output bounds |
| `adapters/shared/test/test_hooks.py:50`, `:132`, `:150` | Existing expectations that permit continued/error warnings |
| `adapters/{claude-code,codex}/hooks/hooks.json` | Native registration, matchers and timeout |
| `adapters/shared/test/test_packages.py:17` | Installation without the source checkout |
| `adapters/pi/src/completion.ts:92`, `:142` | Settlement and bounded controlled completion reference |
| `adapters/pi/src/acceptance.ts:54`, `adapters/pi/src/candidate.ts:111` | Exact required verdict and candidate contract |
| `adapters/pi/completion.md` | Supported entry point and same-account limits |

Use Graft before source discovery; source takes precedence over graph summaries.
Prefix shell commands with `rtk` (`rtk proxy` for unsupported commands). Cargo
operations use `--locked` and the repository's existing `target/`; serialize
operations per target. Installation and harness probes isolate HOME, XDG and
harness configuration. Preserve the pre-existing `.codex/hooks.json` edit; do
not stage or overwrite it. Never use the developer's live trust/hook store.
Remove task scratch using the cleanup skill, keeping normal iterative `target/`
and intended release packages. Do not upload/tag/publish or change catalog pins
as a side effect of this planning or implementation packet.

## Decision Inventory

| # | Decision | Explicit or implied? | Where in plan |
| --- | --- | --- | --- |
| 1 | Limit work to the two existing native adapters | explicit | Required outcome and scope |
| 2 | Completion authority must deny every non-pass | explicit | Required outcome and scope |
| 3 | Prove each host, then select native or controlled entry point | explicit | N0 |
| 4 | Retain v1/schema-7 and owner-selected requirements | explicit | Required outcome and scope; N1/N2 |
| 5 | Share pure verdict/decision logic, separate host I/O | explicit | N1/N2 |
| 6 | Run acceptance synchronously; no reusable pass cache | explicit | N2 |
| 7 | Scope any lifecycle state to its real owner and attempt | explicit | N2 |
| 8 | Bound repair and cancellation with one monotonic deadline | explicit | N2 |
| 9 | Bind a completion claim to settled candidate inputs | explicit | Required outcome and scope; N0/N1 |
| 10 | Qualify exact routes/versions and isolate any new launch contract | explicit | N0; N3 |

## Irreversibility Triage

| # | Decision | TWO-WAY / ONE-WAY | Isolation strategy |
| --- | --- | --- | --- |
| 1 | Two native adapters | TWO-WAY | n/a — internal work scope |
| 2 | Deny non-pass completion | TWO-WAY | n/a — internal decision behavior before publication |
| 3 | Evidence before delivery choice | TWO-WAY | n/a — evidence precedes implementation selection |
| 4 | Preserve v1/schema-7 | TWO-WAY | n/a — preserve the existing published contracts |
| 5 | Pure decisions, separate host I/O | TWO-WAY | n/a — internal module boundary |
| 6 | Synchronous fresh acceptance | TWO-WAY | n/a — no persisted approval contract |
| 7 | Attempt-scoped lifecycle state | TWO-WAY | n/a — lifecycle state is internal; its design precedes code |
| 8 | Bounded repair/deadline | TWO-WAY | n/a — internal bounds with explicit incomplete outcome |
| 9 | Settled candidate identity | TWO-WAY | n/a — reuse the existing candidate contract |
| 10 | Qualified launch/support contract | ONE-WAY | Independently version adapters; qualify and document a new entry point before publication, keep ordinary sessions intact, make installation explicit, and retain older packages/entry points for explicit rollback. |

## Fork Audit

### Sync call vs queue/background job

- Inventory row: #6; chosen sync condition: “The caller must abort if the work
  fails” — YES, the completion invariant withholds success for non-pass.
- Opposing condition: “the work calls an external service whose
  latency/availability you don't control” — YES for a possible model repair.
- Satisfied: YES — N2 explicitly rejects background acceptance because it loses
  authority over the response; bounded model failures end as incomplete.

### Normalize vs denormalize

- Inventory row: #6; chosen normalize condition: “the duplicated fact feeds a
  correctness decision” — YES, cached acceptance could authorize stale inputs.
- Opposing condition: “staleness is tolerable with a bound the plan states” — NO,
  every completion needs a fresh complete pass.
- Satisfied: YES — no reusable pass cache or duplicate approval store.

### Where state lives (URL vs client memory vs server session vs DB)

- Inventory row: #7; chosen condition: “per-user but disposable” — YES, repair
  lifecycle is attempt-scoped, and restart requires fresh acceptance.
- Opposing condition: “must outlive the session or be visible to other
  users/devices” — NO, N2 forbids persistent approval/repair state.
- Satisfied: YES — N2 requires a real state owner before cross-callback logic;
  it does not pretend independent hook subprocesses share memory.

### Config vs code

- Inventory rows: #2/#4; chosen code condition: “The value is an invariant other
  code silently assumes” — YES, only a complete matching acceptance pass succeeds.
- Opposing condition: “An operator must change the value without a deploy” — NO,
  scope explicitly excludes a force-pass switch.
- Satisfied: YES — owner policy selects checks but cannot reclassify an error.

No fork match: rows #1, #3, #5, #8, #9, #10; internal boundaries, runtime proof,
deadline testing, candidate identity and launch qualification do not choose a
service topology, new dependency, database migration, deletion or stream system.

## Corner Scan

- Unmigratable schema: ABSENT — N2 preserves schema 7 and N0 isolates any new launch contract before implementation/publication.
- Side effects without idempotency keys: ABSENT — fresh acceptance intentionally reruns commands under the existing ABI; N2 serializes one completion attempt and adds no retried publication effect.
- Test-hostile boundaries: ABSENT — N1 requires pure decision tests alongside actual host consumption tests.
- Auth/tenancy bolted on later: ABSENT — owner-selected policy and completion authority are in scope from the start, with no hosted multi-tenant service.
- Unbounded growth: ABSENT — N2 forbids persistent approval state, bounds repair, and requires artifact ownership/cleanup.
- Hidden fan-out: ABSENT — scope fixes two harnesses and N2 bounds serial repair/evaluation attempts.
- Shared mutable state across workers: ABSENT — N2 requires an explicit lifecycle owner, attempt identity and concurrent invocation behavior before any callback state is introduced.
- Clock in the logic: ABSENT — N2 requires one injectable monotonic clock/deadline source for new budget logic.
- Hard external coupling, no failure mode: ABSENT — N0/N3 leave unavailable runtime evidence pending and N2 ends model/evaluator failures as incomplete under a total deadline.
- “Pagination later”: ABSENT — no list API or unbounded retained event store is introduced.

## Pre-Mortem

1. **Stop never arrives or another hook permits completion.** Mechanism: a
   disabled registration or competing hook bypasses the expected callback.
   First symptom: failing work finishes without an IronLint decision.
   Vulnerable transition: N0 native route selection; require its missing-event
   probe and a controlled route or explicit unsupported conclusion.
2. **A second installation starts duplicate repair loops.** Mechanism: a native
   plugin plus local registration dispatch two Stop hooks for one attempt.
   First symptom: duplicate repair prompts and inconsistent exhaustion.
   Vulnerable transition: N2 installation; require N1's duplicate-registration
   case and N2's single explicit completion path.
3. **A clean candidate is replaced after checks.** Mechanism: a queued message
   or detached terminal mutates source between evaluation and host success.
   First symptom: completed bytes differ from the evaluated input.
   Vulnerable transition: N0/N2 finalization; require N1's late-mutation test and
   settlement/candidate proof before the entry point receives a completion claim.

## Verdict

VERDICT: PASS

The plan is ready to start with N0; this verdict is about the handoff, not proof
that either native adapter can already enforce completion.

Findings that survived the discard rule: NONE.

DISCARDED: missing/conflicting Stop bypass — N0 requires real route proof and an
unsupported/controlled conclusion instead of trusting callback delivery.

DISCARDED: duplicate repair loops — N1 tests duplicate registration and N2
requires a single explicitly installed completion path.

DISCARDED: late-write acceptance race — N0/N1 require settlement and candidate
identity proof; N3 forbids claiming native protection without that evidence.

Required edits: NONE; these mitigations are already included in N0–N3.
