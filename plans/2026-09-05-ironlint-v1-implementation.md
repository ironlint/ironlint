# V1 implementation plan

## Resume here

- **Target:** a breaking v1 release with external acceptance and editable feedback.
- **Status:** core/CLI implemented; integration, feedback, and removal work remain.
- **Verified code baseline:** `459bbadf80e2a81f4fba1fd39a57aa4c65fdda45` on `main`.
- **Baseline evidence:** 1,048 locked Rust tests; clippy and fmt passed; 94.25%
  aggregate region coverage with every gated file at least 80%; all four adapter
  suites passed. One independent review of that fix batch was resolved. These are
  code checks, not live v1 acceptance or feedback evidence.
- **Current task:** P0, select and prove the reference platform/harness contracts.
- **Access:** platform, harness, versions, and disposable-repository access have
  not been selected or verified. Do not infer a blocker until inspected.
- **Contract:** [v1 specification](../specs/2026-09-05-ironlint-v1-design.md).
- **Source map:** [current architecture](../docs/architecture.md).

## Scope decisions

V1 is a clean break. Remove the unversioned execution path and schema-6 consumers;
do not build backward compatibility, an automatic config converter, a migration
report command, or coordinated version rollback. Old configurations get a clear
unsupported-format error and a pointer to current authoring instructions.

Safe removal of installed IronLint entries remains required. Preserve unrelated
hooks/settings and user edits; never strand a hook calling a removed executable.
Use existing installer ownership machinery. No compatibility framework is needed.

Ship one external acceptance integration and one live-verified feedback adapter.
No new service, broker, daemon, verdict cache, scheduler, or extra harness is in
scope. Freeze expansion of gate-bash, proposal simulation, telemetry, watch, and
self-update. Remove obsolete docs instead of maintaining historical roadmaps.

## Work and dependencies

| ID | Work | Depends on | Status | Evidence |
| --- | --- | --- | --- | --- |
| P0 | Select platform/harness and prove available contracts | None | Ready | Not recorded |
| P1 | External acceptance integration | P0 authority proof | Pending | Not recorded |
| P2 | Completed-edit feedback adapter | P0 harness audit | Pending | Not recorded |
| P3 | Owned-install cleanup and removal of old execution paths | P1 and P2 proven | Pending | Not recorded |
| P4 | Integrated release verification and documentation | P3 | Pending | Not recorded |

P1 and P2 can proceed independently after their P0 prerequisites are satisfied.
P3 inventory can happen earlier; installed blocking paths are removed only with a
tested cleanup procedure and truthful capability documentation.

### P0: next executable packet

**Read:** spec §§4, 9, 14; existing CI in `.github/workflows/`; adapter registry and
selected `adapters/<harness>/` sources. Use graph tools to locate exact callers.

1. Inspect available platform/harness access without changing repository settings.
2. Choose one actual repository platform and one feedback harness from accessible
   evidence. Audit the harness using `adapter-drift-audit`; use current primary
   documentation for external contracts. Do not invent events or workflow syntax.
3. Record the protected operation, every route to it, revision binding, policy
   authority, evaluator provenance, and isolation of candidate execution from
   result publication. Prefer native platform features; do not build a broker.
4. Prepare a disposable-repository proof using simple pass/fail commands. Attempt
   failing acceptance, policy/evaluator replacement, forged success, stale success
   after another revision, and an alternative route to the protected operation.
   Test changed merge inputs if the chosen boundary accepts merged state.
5. Capture a real completed-operation event and its diagnostic delivery behavior.
   Existing pre-write captures do not establish completed-edit support.

**Done:** selected versions, setup, exact commands, denied operation/revision,
and evidence location are recorded below. A mock does not prove enforcement.
If access is unavailable, prepare the concrete setup first, record the missing
capability, and request only the access needed. Continue independent work.

**P0 decisions/evidence:** not yet recorded.

### P1: external acceptance

Use the existing CLI and approved policy/evaluator against an exact materialized
candidate. Build the smallest consumer requiring exit 0, schema 7, `event: accept`,
complete `pass`, no error/unrun checks, and the expected required check-ID set.
Keep publication credentials outside candidate process/environment/filesystem
access. Bind success through the platform's native revision mechanism.

**Done:** repeat P0 denial tests using IronLint, including missing evaluator,
malformed JSON, execution error, and changed merge inputs where supported. Record
the actual protected operation and revision denied, not just a local exit code.
Document installation, policy updates, privileges, and supported boundaries.

### P2: feedback adapter

Own only the selected adapter and its contract tests. Translate a completed edit
or batch to `change`, including deletions and rename endpoints. Unknown paths run
all change checks. Report failures/errors and a reproduction command while keeping
edits. Suppress routine successes. Run synchronously within the configured budget;
label results superseded if newer edits are observed. No proposal reconstruction
or `gate-bash` call in the new path.

**Done:** provenance-stamped live payloads and visible red → repair → green proof,
bulk edit, missing executable, and unsupported mutation subsequently rejected by
full acceptance. Publish tested events, delivery channel, and coverage limits.

### P3: remove obsolete execution and installation paths

Trace callers in CLI, core, adapters, installer templates, tests, scripts, and docs
before deletion. Use existing ownership records to remove/replace only IronLint
registrations; show a concrete manual action where ownership is ambiguous.
Test chained/user-edited hooks and a repeated cleanup in a temporary home/repo.

Delete the Bash-gate crate/command, unversioned dispatch, schema-6 result consumers,
preview stdin/per-file ABI, `extends`/`steps`/suppression execution, unused diff
selection, and default floor installation when no retained caller needs them.
Update `init` to create v1 config and install only proven support. Unsupported
adapters must not keep registrations calling deleted commands.

Retain useful validation, scope matching, execution, consent, and diagnostics.
Choose the smallest treatment of telemetry/watch: retain only if meaningful for
v1, otherwise remove with a clear release note. Do not redesign them. Update the
schema/installed authoring guide, help, doctor, README, and architecture alongside
the code. Remove obsolete-only tests after identifying retained-contract coverage;
do not weaken tests of behavior v1 still requires.

**Done:** no installed IronLint entry calls a removed command; unrelated settings
survive; v1 examples run; old config is rejected clearly; no current doc promises
unavoidable local enforcement. No automatic conversion or rollback is required.

### P4: release evidence

Record each result with the tested commit, platform/harness version, exact command
or procedure, expected/actual result, and a small fixture/artifact link. Store live
adapter payloads in the selected adapter's existing fixtures directory. Use one
`tests/evidence/v1/README.md` only when actual external proof exists; no empty
evidence scaffolding. Keep the evidence index here current.

| Required proof | Owner | Current evidence |
| --- | --- | --- |
| Editable red state, visible feedback, red denied and green accepted | P1/P2 | Missing live proof |
| Nonzero command and missing/broken evaluator deny acceptance | P1 | Core tests only |
| Stale revision, partial-staging mismatch, changed merge inputs denied | P1 | Missing platform proof |
| Candidate cannot replace policy/evaluator or forge accepted results | P1 | Missing platform proof |
| Unsupported write is caught by full acceptance | P1/P2 | Missing live proof |
| Batch runs once per selected check; acceptance includes every check | P2/P4 | Core tests; live batch proof pending |
| Output caps, total deadline, pipe-holding descendant cleanup | P4 | Baseline core tests; rerun on release tree |
| Owned-entry removal preserves unrelated hooks and leaves no dead calls | P3 | Cleanup proof pending |

Run `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`,
`cargo fmt --all --check`, `bash scripts/ci-coverage.sh`, and
`bash scripts/ci-adapters.sh` against the integrated tree. Use a temporary
`XDG_CONFIG_HOME` for adapter tests; never exercise install/trust tests against the
developer's live configuration. Keep per-file region coverage at least 80%.

Request one separate review of each integrated implementation batch. Reopen a
resolved batch only for a newly reproducible regression. Final review covers
authority bypass, schema consumers, installation cleanup, and doc/code agreement.
Publishing is a separate action requiring authorization; do not infer it from a
green test suite. Do not declare v1 complete while live proof is missing.

## Execution discipline

- Expand only the next ready packet. Assign exclusive files if delegation is used;
  never paste the full project history into a worker prompt or create duplicate plans.
- For each bug: failing regression → smallest shared root-cause fix → focused test.
  For new behavior, pin the acceptance test before implementing it.
- Run one Cargo build/test/coverage operation at a time per target directory.
- Update this resume block, status row, and evidence after an integrated batch.
  Mark unavailable evidence explicitly; do not convert stale checkboxes into proof.
- Refresh code graphs after substantial code changes and remove task-created
  transient artifacts. Preserve unrelated working-tree changes.
