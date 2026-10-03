# Stronger coding-harness compliance

Status: **Implementation integrated; live-model qualification remains pending.**
Date: 2026-10-01.
Baseline: `feature/durability-simplification` at `dd04068f3092e7d87ded98b99ba01c8f569f17b6`.

The separately requested
[native completion implementation](2026-10-02-native-completion-enforcement.md)
owns the Claude Code/Codex completion follow-up. Its explicit controllers and
pinned real-runtime evidence are separate from the existing feedback/Stop-repair
packages described below. Ordinary native sessions remain feedback routes.
Pi's pending H3/H5 work and live model utility measurements remain here.

## Outcome and scope

Make following executable project rules a property of the coding workflow:
edit, receive useful feedback, repair, and complete only after all required
checks pass against the candidate being completed. Intermediate failures remain
editable, including TDD's red phase and incomplete multi-file refactors.

This is an adapter-owned follow-up to the completed v1 and durability plans.
It borrows OpenAPPA's enforced decision point, actionable recovery, behavioral
policy tests, and reusable rule examples. It does not add information-flow
tracking, general agent permissions, model-based verdicts, or a security service.
The checkboxes below track the implementation and qualification separately.
An unchecked packet remains outside the completion-integration release claim.

The first supported completion integration is Pi, the branch's sole active
adapter. Deliver one qualified path before expanding the harness matrix.
Keep the Rust evaluator independent of Pi and keep live qualification outside
core release requirements.

## Authorized adapter packaging follow-up — 2026-10-02

The maintainer authorized independent adapter packages in this repository and
new Codex and Claude Code integrations. Move installation/ownership out of
`ironlint-core` into an independently versioned `ironlint-adapters` crate.
Keep `init`/`doctor` as CLI facades. Load adapter scripts, skills, and hook
registrations from separately distributed files at installation time; no
harness executable is compiled into the evaluator or CLI. Preserve owned
legacy cleanup, locks, interrupted-install recovery, foreign files, and user edits.
Package roots are explicitly selectable with `IRONLINT_ADAPTERS_ROOT`; release
installs use adjacent adapter files, while development builds use this checkout.

Codex and Claude Code packages provide synchronous post-edit change feedback
and fresh full acceptance at Stop. They share bounded subprocess execution and
strict schema-7 validation. Native Stop hooks request repairs; they do not
claim immutable candidate completion or publication enforcement. Persistent
failures during an already continued Stop produce an explicit incomplete
warning and stop automatic repair churn. Pi's controlled completion contract
and provenance-stamped fixtures remain intact. Unknown mutation paths run all
change checks; no old pre-edit command or LLM evaluator is restored.

### Decision Inventory

| # | Decision | Explicit or implied? | Where in plan |
|---|---|---|---|
| 1 | One repo, independent adapter versions | explicit | Packaging follow-up |
| 2 | Installer separated from evaluator | explicit | Packaging follow-up |
| 3 | Scripts and registrations loaded at runtime | explicit | Packaging follow-up |
| 4 | Package root selected outside hook payload | explicit | Packaging follow-up |
| 5 | Retain ownership and recovery records | explicit | Packaging follow-up |
| 6 | Share bounded subprocess/result logic | explicit | Packaging follow-up |
| 7 | Feedback after edits; full acceptance at Stop | explicit | Packaging follow-up |
| 8 | Stop continuation is a local workflow aid | explicit | Packaging follow-up |
| 9 | No persistent pass cache or repair counter | implied | Fresh acceptance and Stop input |
| 10 | Keep retired-install cleanup | explicit | Packaging follow-up |

### Irreversibility Triage

Rows 1–4 and 6–9 are TWO-WAY: internal packages and native registrations can be
changed independently; the v1 policy and schema-7 ABI stay fixed. Rows 5 and 10
are ONE-WAY persisted installation state: preserve existing sidecar framing and
read old registrations alongside new ones, removing only owned entries.

### Fork Audit

- Migration strategy, rows 5/10: choose expand-contract because “Real data exists
  in production”; old registrations remain recognizable alongside new events,
  rejecting big-bang because installed hooks and edited artifacts already exist.
- Sync/background, rows 6/7: choose sync because “The caller must abort if the
  work fails”; the Stop response needs acceptance diagnostics. Reject background
  dispatch because its result could arrive after the turn ends. Bound the run.
- State storage, row 9: choose the “shortest-lived store that satisfies the
  requirement”; fresh evaluation and the host's continuation flag need no DB.
- Config/code, rows 3/4: choose package config because the maintainer must update
  registrations “without a deploy” of the evaluator; ownership, package version,
  and reinstall provide audit/rollback. Keep result validation invariants in code.
- One service/split, rows 1/2: use internal package boundaries, not a network
  service; independent harness maintenance needs no shared remote transaction.
- No other fork matches: result transport and local capability claims, row 8.

### Corner Scan

- Unmigratable schema: ABSENT — retain the existing sidecar and owned legacy cleanup.
- Side effects without idempotency keys: ABSENT — installer hashes and locks preserve idempotent retries.
- Test-hostile boundaries: ABSENT — pure payload/result functions plus isolated CLI and subprocess tests.
- Auth/tenancy bolted on later: ABSENT — no service or tenancy; policy consent remains unchanged.
- Unbounded growth: ABSENT — no pass cache or persistent repair history; subprocess buffers are bounded.
- Hidden fan-out: ABSENT — one evaluator invocation per delivered hook; no agent fan-out.
- Shared mutable state across workers: ABSENT — retain installer resource locks; hooks share no approval state.
- Clock in the logic: ABSENT — subprocess deadlines use one monotonic clock and injectable test limits.
- Hard external coupling, no failure mode: ABSENT — missing binaries, malformed results, deadlines and missing packages report incomplete work.
- Pagination later: ABSENT — no list service or unbounded external collection is introduced.

### Pre-Mortem

1. Package updated but installation stayed stale: runtime package/installed hashes
   differ; doctor reports the difference and reinstall updates owned artifacts.
2. New write tool never delivers the registered event: no early feedback arrives;
   Stop still runs all checks, with tool coverage explicitly limited in each README.
3. Another Stop hook overrides continuation, or background writes race checks:
   native completion remains host-owned; document local feedback limits and retain
   the separately qualified Pi candidate-binding runner for stronger claims.

### Verdict

VERDICT: PASS WITH CHANGES
Required edits: implement runtime package loading and stale-install diagnostics;
keep full acceptance independent of edit delivery; label native Stop behavior as
local feedback/repair; test continued Stop failures without a persistent pass cache.
No human decision or unisolated published contract is introduced by this scope.

### Implementation and validation evidence

Implemented from `025c8fc` and verified before committing. Adapter tags and
external package publication remain pending. Codex and Claude Code
archives are independently versioned at 0.1.0. The adapter package workflow
builds archives without a Rust build. Runtime installation from both extracted
archives was verified with isolated home/config directories.

- `cargo test --locked --workspace`: passed. The final coverage run also reran
  the complete workspace, including the registration-preview regression.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo fmt --all --check`: passed.
- `bash scripts/ci-coverage.sh`: passed; workspace region coverage 93.93%, every
  Rust file at least 80%.
- Native hook contracts and self-contained packages: 14 tests passed, including
  real CLI red/repair, fresh acceptance, consent failures, process limits, and
  closed-pipe timeouts. Both release archives install both lifecycle events.
- Pi typecheck and all 63 tests passed with test-file concurrency set to one.
  Two prior parallel suite runs exceeded the existing two-second child-startup
  guard. Serial scheduling retains all timing assertions and stamped fixtures.
- Acceptance verifier conformance and all nine check-recipe fixtures passed.
- `bash tests/e2e/features/run.sh`: local Docker feature suite passed. The build
  stage needs only the generic authoring guide; harness scripts are runtime files.
- Separate review found cross-scope migration, missing Stop diagnostics, and
  an uncaught closed-pipe timeout. All three were fixed; focused recheck found
  no remaining issues. The preview now discloses every affected scope/event.

Native compatibility evidence is official hook documentation checked on
2026-10-02, observed local CLIs (Codex 0.159.1, Claude Code 2.1.207), synthetic
payload fixtures, and isolated real IronLint integration. No live model session
or Windows descendant-cleanup qualification is claimed. Pi's prior live-model
qualification remains pending as recorded in H3; this follow-up does not alter
that claim. Publication destination remains an operator choice.

### Marketplace distribution follow-up — 2026-10-02

Reconciled `.claude-plugin/marketplace.json` with the independently versioned
Claude Code plugin. The marketplace keeps its `ironlint-marketplace` identity,
advances its catalog version to 0.3.0, and maps legacy `ironlint` installs to
`ironlint-claude-code`. Plugin version 0.1.0 comes from `plugin.json`; the catalog
does not override it. Descriptions now state post-edit checks and local Stop
repair, and the owner/homepage match the current `ironlint/ironlint` origin.

The prepared catalog source is `ironlint-claude-code-plugin-0.1.0.zip`, an HTTPS
release ZIP under
`adapter-claude-code-v0.1.0`, pinned to the exact locally built archive's SHA-256.
Both helpers and the authoring skill live inside the selected plugin directory.
The legacy source-folder catalog reproduced a missing `shared/hooks/hook.py`
failure when only that directory was cached. The new regression copies only the
packaged plugin and successfully runs both PostToolUse and Stop hooks.

Focused review also identified archive-root discovery risk: the installer ZIP
contains both harness and shared directories, while the native archive loader
expects plugin content at root or inside one wrapper. `pack.py --plugin-only`
now emits root-level plugin content and omits the shared sibling; the catalog
selects that artifact. The regular installer ZIP remains unchanged in layout.
The regression rejects multiple wrappers before simulating the cache copy.

- Native suite: all 16 tests passed, including isolated real CLI integration
  and native plugin-root/dependency coverage.
- Catalog pin and packaged name/version/description/homepage were checked
  against the exact release ZIP; `target/adapter-packages/SHA256SUMS` was refreshed.
- Packaged plugin manifest validation passed with the observed Claude Code
  2.1.207 CLI, using temporary HOME/XDG/Claude configuration directories.
- Archive catalog validation on that old CLI fails at `plugins.0.source`, as
  expected: the official archive source requires Claude Code 2.1.224+.
  Marketplace download/installation and live-model qualification remain pending.
- Workflow YAML parses, catalog edits trigger the package-contract workflow,
  and `adapter-*` tags bypass the evaluator's cargo-dist release workflow.
- Separate focused review identified the archive-layout risk above; the
  plugin-only fix and final artifact recheck have no remaining findings.
  An isolated Claude Code 2.1.224 listing attempt timed out without output,
  produced no qualification evidence, and its temporary files were removed.

The maintainer authorized committing and pushing this implementation to
`origin/main` on 2026-10-02. Adapter tags and release uploads remain pending.
The GitHub Release URL is a prepared destination; final package publication
destination remains an operator choice. Upload the exact pinned artifact or
update the pin after rebuilding it. Older clients can use local registration
from extracted packages.

## Resume block

- [x] H0 — Qualify the Pi completion contract and freeze the supported entry point.
- [x] H1 — Implement controlled completion and bounded repair.
- [x] H2 — Make feedback useful for repairing specific rules.
- [ ] H3 — Prove compliance through full coding scenarios and real Pi delivery.
- [x] H4 — Ship a small set of check examples with positive/negative fixtures.
- [ ] H5 — Integrate documentation, qualification evidence, and release checks.

Start with H0. H1 depends on H0; H2 can follow its contract decision. H3 exercises
H1 and H2 together. H4 uses the feedback convention from H2. H5 closes the whole
slice. Request a separate agent review after each integrated implementation
batch, fix concrete findings, and record tested commits in the evidence ledger.
Do not mark a packet complete from mocked lifecycle tests alone when it requires
real harness evidence.

## Existing behavior to reuse

| Surface | Current implementation | Work this plan adds |
| --- | --- | --- |
| Policy and selection | `crates/ironlint-core/src/config/v1.rs`: every check runs at accept; change triggers select early feedback | No new policy language or selection semantics |
| Execution | Core runner, immutable policy snapshot, drift checks, cancellation and bounded processes | Reuse the CLI, including incomplete/error outcomes |
| Pi feedback | `adapters/pi/src/index.ts`: supported post-edit events, cancellation, schema-7 validation, reproduction command | Consistent repair guidance and a distinct completion integration |
| Acceptance consumer | `scripts/verify-acceptance.sh` and its tests | Reuse its expected-check-set and complete-result contract; retain exact outcome validation |
| Candidate evaluation | `tests/e2e/features/driver.sh`, `scripts/run-containerized-acceptance.sh` | Apply existing candidate/policy separation to the qualified Pi completion path |
| Behavioral coverage | `tests/e2e/features/` already covers red/repair, unsupported writes, candidate commits, incomplete runs | Real Pi delivery and complete/repair lifecycle coverage |
| Recipes | `docs/writing-checks/recipes.md` | Runnable examples with tests of the actual rule |

The active Pi tests use synthetic `tool_result` inputs. Existing fixtures record
retired Pi 0.84.3 `tool_call` events; they cannot qualify the current adapter.
The documentation names Pi 0.85.1 for feedback, but this plan does not assume it
has the completion capabilities available in upstream main.

## Fixed design decisions

1. Keep `version: 1`, schema 7, existing CLI exit codes, the check ABI, and the
   exit-code-owned verdict. No additional rule engine, suppression, force-pass,
   check-specific acceptance filter, or parsing of diagnostic prose for decisions.
2. Completion is a host-owned successful task result for one candidate. An
   assistant saying "done", a local commit, a post-edit pass, or a previously
   saved verdict is not that operation. The ordinary Pi extension remains
   feedback-only unless its exact host contract qualifies for more.
3. Implement the completion controller under `adapters/pi/`, outside the Rust
   evaluator. Use a small Pi SDK runner as the default delivery path. A native
   completion veto may replace it only if H0 proves the same settlement,
   candidate, and failure behavior. Do not turn the controller into a scheduler,
   daemon, hosting service, or general multi-agent runtime.
4. Use a fresh acceptance evaluation for every completion attempt. Keep lifecycle
   state in the running integration; do not introduce a persistent approved/dirty
   database or reusable pass cache into core. Restart/resume re-evaluates.
5. Keep the developer-selected policy, required check IDs, evaluator identity,
   and runner configuration outside the candidate's choice. Required rule scripts
   and their declared enforcement dependencies also need an owner-approved source;
   a trusted YAML file invoking a candidate-replaceable script does not establish
   that rule. Deliberate use of candidate-owned tests is documented as such. Local
   execution consent remains consent; do not call `ironlint trust` in the repair loop.
6. Preserve current feedback and manual CLI workflows. Installing a new completion
   path is explicit; it does not silently replace ordinary Pi sessions or revive
   retired harness adapters. A completion result cannot claim merge authority.

### Candidate and authority boundary

The controlled runner must identify what it completed. Its first supported
workspace type is a Git repository. Capture a private candidate from tracked
files and nonignored untracked files, including deletions and executable modes,
without staging, committing, resetting, or stashing the user's repository.
Use a private index/object store or equivalent immutable export; exclude `.git`
and declared scratch outputs. Reject unsupported symlinks, submodules, and special
files explicitly in this first path rather than silently omitting inputs.

Run acceptance against that captured tree. Return its content identity and retain
its exact artifact until the caller takes ownership. The output refers to that
artifact, never to whatever bytes happen to remain in the mutable working tree.
The caller must declare any ignored source inputs required by the selected rule
profile. Include those declared files in capture and identity; reject missing or
unsupported declared inputs before evaluation. Do not infer arbitrary shell-script
dependencies. Rules relying on undeclared inputs are outside the supported capture
contract, and each shipped profile tests that its declarations are sufficient.
Build outputs belong in scratch space.

Keep the evaluated source tree read-only where the deployment supports that,
with build outputs directed to separate writable scratch space. Otherwise verify
the evaluation tree against the captured input manifest before and after checks
and reject source drift, even when every command exits zero. The retained artifact
must still have the same identity. Comparing only the original working tree is
insufficient. A local before/after comparison detects persistent drift, not a
malicious write-and-restore; stronger claims require the isolated read-only setup.

Stop host-dispatched mutations before capture and acceptance. Detect edits racing
capture and compare the working input manifest again before returning success;
a changed input invalidates the attempt. Repeated concurrent edits exhaust the
same completion budget and report incomplete work. Do not claim that manifest
checks alone make arbitrary outside writers or detached processes safe.

H0 must distinguish two claims in its capability record: local workflow completion
with stated same-account assumptions, and enforced acceptance under all six
requirements in section 4 of the v1 contract. A local SDK process does not create
an independent security boundary. Reuse a caller-owned isolated workspace and the
existing container boundary where the stronger claim is made; do not build an OS
sandbox. If the supported deployment cannot prevent candidate code from tampering
with policy, evaluator, controller, or result recording, document that limitation
and do not advertise tamper-resistant enforcement. Keep the core promise unchanged.

## H0 — Qualify and freeze the Pi contract

**Read:** `adapters/pi/{README.md,package.json,src/index.ts}`, the active tests,
`adapters/pi/fixtures/README.md`, and v1 spec sections 4 and 9. Use the context
graph before following implementation symbols.

**Work:**

- Inspect the exact released Pi package and primary SDK/extension documentation.
  Pin one runtime version and source commit for the new integration. Record
  supported platforms, session settlement, queued work, retries, user abort,
  tool completion, resume, extension loading, and cleanup semantics.
- Establish the missing Pi reference for the repository's `adapter-drift-audit`
  skill: only `references/claude-code.md` exists at the baseline. Add the Pi
  source map, then perform the read-only audit; write a verified watermark only
  after evidence exists. Do not assume the skill already covers Pi.
- Prove how the controller observes full settlement and withholds its own success
  result. Current upstream SDK docs distinguish `agent_end` from `agent_settled`;
  verify the chosen release rather than copying an event name from main.
- Prefer a small SDK runner that owns task input, session lifetime, finalization,
  exit status, and the accepted candidate artifact. Show that model text and
  extension-generated follow-up messages cannot independently mark it complete.
  If a native veto is chosen instead, document every supported completion route.
- Freeze the controller's invocation, candidate capture rules, result channel,
  policy source, check execution boundary, input manifest, and resource budgets
  in `adapters/pi/completion.md`. Specify installation and failure behavior when
  the runtime version is unsupported. Do not widen the feedback peer dependency
  to imply that untested releases support completion.

**Evidence:** a minimal real-runtime probe for clean settlement, queued work,
abort, and failure to initialize; source links pinned to the tested revision;
explicit local-versus-isolated guarantee. Use a temporary home and fixture repo.

**Done when:** one implementation path can withhold success, bind its candidate,
and describe its authority honestly. If no native veto qualifies, implement the
SDK path. If that path cannot meet the contract either, leave H1/H3 pending and
report the concrete limitation; never substitute a prompt-only completion gate.

## H1 — Controlled completion and bounded repair

**Likely files:** new `adapters/pi/src/completion.ts` and `candidate.ts`, focused
`adapters/pi/test/completion.test.ts` and `candidate.test.ts`, Pi package entry
points, and a small documented launcher. Factor shared subprocess/verdict code
from `src/index.ts` only where both consumers need it. No new Rust CLI command.

**Flow:**

```text
work -> settle -> capture candidate -> full accept
                                       | pass -> complete exact candidate
                                       | violation -> repair -> settle again
                                       | error/cancel/budget -> incomplete
```

- Run acceptance outside model tool dispatch, even if no tracked edit event fired.
  Cancel/supersede outstanding change feedback before finalization. Await all
  host-owned tool work and reject unsupported detached-write configurations.
- Consume process status and one well-formed schema-7 document. Require event
  `accept`, status `pass`, no error or unrun checks, exactly the trusted expected
  check IDs with no duplicates, and valid passing outcomes/zero command statuses.
  Unknown schemas, missing/extra IDs, truncation and malformed bytes fail closed.
  Share conformance cases with `scripts/test-verify-acceptance.sh` so consumers
  cannot diverge silently. Never use the permissive diagnostic renderer as a
  decision parser.
- On a violation, return the check diagnostics to the agent as integration feedback
  and allow repair edits. Start another fresh capture/evaluation after settlement.
  Do not automatically alter policy, grant consent, remove failing tests, or run
  a check-supplied repair command. Guidance is for fixing the candidate.
- Default to at most three automatic repair turns and a 900-second controller
  deadline; allow the caller to choose stricter/larger finite budgets through
  runner options. These are integration limits, not new YAML execution settings.
  Include model work, capture, checks, and repair in the total controller budget.
  Keep core command/total deadlines and existing feedback process limits intact.
- Evaluator errors, missing consent, missing tools, and policy drift report
  incomplete work without automatic repair churn. Exhaustion/cancellation returns
  a non-success outcome, retains useful diagnostics, and reaps owned processes.
  An explicit new user attempt starts fresh; no previous pass is reusable.
- Isolate model progress from the controller result channel. Buffer the proposed
  final answer until evaluation succeeds, or label streamed text as unverified.
  Emit one host-owned terminal result containing status and candidate identity.
  On resume, reload trusted configuration and evaluate again before success.

**Tests first:** failing candidate refuses completion; repaired candidate passes;
pass followed by a later edit re-evaluates; missing binary/malformed output cannot
succeed; abort and budget exhaustion terminate; candidate policy substitution and
forged model success text do not replace the host decision. Cover dirty files,
untracked files, deletion, partial staging, racing writers, unsupported entries,
and artifact lifetime without changing the user's index or branch. A check that
edits evaluated source and then exits zero must not authorize completion; verify
that supported build outputs in scratch space do not invalidate valid work.

**Done when:** a controlled Pi run can finish only with complete evidence for the
returned artifact, while repair remains possible. No claim is made for ordinary
Pi sessions outside the selected entry point.

## H2 — Actionable, bounded feedback

**Files:** `adapters/pi/src/index.ts`, shared rendering extracted for H1,
`adapters/pi/test/index.test.ts`, `docs/writing-checks/README.md`, recipes, and
the installed `adapters/shared/ironlint-config/SKILL.md` authoring guidance.

Standardize check output as human/agent-readable guidance: violation location,
required pattern, relevant project reference, and a concrete explanation of what
to fix. The adapter adds the stable check ID, event, and reproduction command.
This is an output convention, not a required parser or new config field.

The current renderer selects `reason || stderr || stdout`; preserve useful
stdout and stderr when both exist instead of dropping one. Separate violations
from execution failures, mark truncation, preserve cancellation/supersession
semantics, and suppress routine successful change feedback. Cap the combined
rendered message; retain per-check identification and explain where full output
can be reproduced when many checks fail.

Never execute text copied from diagnostics or treat it as a new instruction with
user authority. Keep malicious/irrelevant output as check output. Reproduction
must use safe argument quoting. Suggestions cannot change the verdict or permit
skipping a required check. If existing output is sufficient, do not introduce
structured remedy IDs, automatic fix commands, or metadata for symmetry with APPA.

**Tests:** both streams retained, multiple failures, actionable example output,
empty diagnostics, execution errors, output caps, invalid UTF-8, unusual paths,
and stale results. Keep existing subprocess/cancellation cases.

**Done when:** a failure tells the agent which rule failed and how to investigate
or repair it, and a subsequent exit-code evaluation alone determines success.

## H3 — Harness compliance scenarios

**Files:** new adapter-owned `adapters/pi/test/compliance/` scenarios and runner,
`adapters/pi/fixtures/README.md`, a pinned-runtime qualification record, and an
adapter CI lane. Extend the existing core Docker suite only for shared CLI
regressions; do not create a duplicate evaluator test harness.

Use two distinct layers: deterministic scenarios with scripted model responses
through the real pinned Pi runtime and real IronLint binary, then a small live
model task suite that measures repair behavior. Capture actual lifecycle/tool
payloads with runtime version, source SHA, platform, runner commit, and capture
command. Preserve historical fixture provenance. If the runtime cannot use a
scripted provider, record which scenarios need a live model instead of calling
synthetic callback tests real-runtime qualification.

| Scenario | Required result |
| --- | --- |
| Forbidden dependency or architecture edge | Feedback names the rule; completion denied |
| Correct repair | Fresh full acceptance permits completion |
| TDD red, then green | Red remains editable; only green can complete |
| Bash edit with no early feedback | Completion still runs every required check |
| Delete, rename, batch, indirect manifest change | Complete acceptance cannot omit the affected requirement |
| Passing feedback or local CLI check, then another edit | No stale authorization |
| Dirty/untracked/partially staged candidate | Accepted artifact matches the declared capture, not an older commit |
| Queued turn, retry, or late tool result | No completion before full settlement |
| Concurrent edit during capture/evaluation | Invalidate/retry within budget, otherwise incomplete |
| Check changes evaluated source and exits zero | Reject the altered evaluation; scratch-only build output remains allowed |
| Declared ignored input is missing or changes | Refuse capture or invalidate the attempt |
| Missing binary, timeout, malformed/partial JSON, error, consent loss | No successful completion |
| Candidate changes policy or prints a forged verdict | Owner-selected policy/result source remains authoritative |
| No check matches change filters | Full acceptance still runs |
| Repeated unproductive repair, user abort, resume | Bounded stop; resume needs a new acceptance run |
| Unsupported runtime or absent completion integration | Explicit unsupported/incomplete status; no claimed enforcement |

Deterministic release criteria: zero violating candidates completed, every clean
fixture completes, all denial/cancellation cases terminate within documented
bounds, no stale result accepted, and no user index/branch changes from capture.
Exercise the controller's explicit missing-startup paths as well as errors after
startup; an uninstalled ordinary Pi extension cannot magically intercept a task.

For the live suite, use fixed tasks and seed repositories, including a legitimate
TDD task and a multi-file repair. Compare the same model/configuration with
feedback alone and with controlled completion. Record completion rate, violations
accepted, repair turns, check invocations, elapsed time, and available token cost.
Show denominators and failures; label unavailable measurements. Success is
compliant task completion, not number of blocks. Do not impose a statistically
unsupported utility claim or turn provider-dependent runs into a core CI gate.

**Done when:** replayable real-runtime evidence supports the declared integration,
and live repair results expose its practical cost. Missing credentials or runtime
access leave live qualification pending and the supported claim appropriately
limited; synthetic fixtures do not fill that gap.

## H4 — Tested check examples

**Files:** `examples/checks/` with a small catalog, corresponding
`tests/fixtures/check-recipes/`, a fixture runner, and links from
`docs/writing-checks/recipes.md`. Use one reusable fixture runner, not a new DSL.

Ship three initial examples: an architectural import boundary, a forbidden direct
dependency, and consistency of a generated artifact with its declared input.
Choose and document the actual language/parser/tool version for each example;
use existing project tooling where possible. Do not sell text matching as
language-semantic coverage. Format and ordinary test commands remain recipes.

Each example contains plain v1 YAML, reviewed scripts where needed, dependencies,
limitations, useful failure output, and at least one passing, failing, and repaired
fixture. Add boundary cases such as multiline imports or dependency aliases only
where the rule claims to handle them. Missing dependencies/parsing failures must
not silently pass. Verify expected check IDs and outcomes with the real CLI and
an isolated consent store. Never invoke the user's live trust store in tests.

Adoption is explicit copying/materialization and developer review. Do not silently
edit an existing policy, overwrite customized scripts, weaken an existing check,
or bless new code. No remote pack registry, automatic pack updates, `extends`,
plugin engine, or automatic rule generation is introduced.

**Done when:** each example catches its intended negative fixture, accepts valid
work, gives useful repair guidance, and can be reproduced from its documentation.

## H5 — Integration, documentation, and qualification

Update the current architecture, v1 integration sections, Pi setup/capability
documentation, check-authoring guidance, and plan index to describe implemented
behavior. Keep the completed v1/durability evidence intact. Documentation must
name the supported entry point, exact runtime/version range, acceptance candidate,
feedback coverage gaps, deployment assumptions, and independent core usability.

Run focused tests after each change. For the integrated slice, run the existing
Pi typecheck/test lane and new deterministic compliance/recipe runners. Run core
workspace tests, clippy, format, coverage, and local Docker features if core/shared
consumer behavior changed; retain at least 80% region coverage per touched Rust
source and cognitive complexity at most 15. Run one Cargo operation per target
directory at a time. Use isolated home/config directories throughout.

Record the final implementation SHA, commands, actual results, reviewer findings,
Pi provenance, and live-model measurements. Clean task-created outputs using the
repository cleanup skill; retain ordinary development caches and user files.
A release of the completion integration requires H0-H5 evidence. It does not
retroactively make live Pi qualification a requirement for the independent core.
Tagging, package publication, and deployments are separate operator actions.

## Evidence ledger

| Packet | Status | Tested commit and evidence | Separate review |
| --- | --- | --- | --- |
| H0 | Complete | `212c427`; Pi 0.87.1 pin, actual SDK startup/settlement/queued/abort probes, source map and capability record | Separate review; findings fixed in commit |
| H1 | Complete | `212c427`; candidate identity, strict accept, bounded repair, cancellation and tool-confinement tests | Separate review; findings fixed in commit |
| H2 | Complete | `212c427`; bounded dual-stream feedback and authoring guidance tests | Separate review; findings fixed in commit |
| H3 | Partial | `212c427`; deterministic real SDK plus CLI scenarios pass. Live comparison has no denominator: configured OpenRouter credential returned 401 before work began | Separate review complete; live evidence pending |
| H4 | Complete | `212c427`; all 9 real CLI positive/negative/repaired recipe fixtures pass in isolated consent | Separate review found no recipe regression |
| H5 | Partial | `212c427`; npm, shell, workspace tests, Clippy, format, ≥80% region coverage, and Docker features pass. Final release record awaits H3 live results | Separate review complete; release qualification pending |

## Sources and follow-up boundaries

- [V1 design](../specs/2026-09-05-ironlint-v1-design.md), especially sections 4,
  9, 14, and 15: candidate binding, authority, integration ownership, and evidence.
- [Durability plan](2026-09-29-durability-simplification.md): completed baseline,
  including subprocess bounds and the limits of synthetic Pi fixtures.
- [OpenAPPA recovery](https://www.openappa.com/how-it-works),
  [policy replay](https://www.openappa.com/validation), and
  [batteries](https://www.openappa.com/batteries): design inspiration only; no
  dependency on OpenAPPA or adoption of its permission algebra is planned.
- [Pi SDK documentation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/sdk.md)
  and [extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md):
  inspected during planning, not pinned runtime evidence. H0 must replace moving
  references with the selected release/source and verify its behavior.

Additional harnesses, managed policy services, general permission handling,
automatic policy maintenance, and automatic code repair are outside this plan.
