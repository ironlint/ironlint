# V1 implementation plan

## Resume here

- **Target:** a breaking v1 core release with deterministic command checks,
  editable feedback, and local feature E2E coverage.
- **Status:** core/CLI implemented; local feature suite added; removal work remains.
- **Verified code baseline:** `459bbadf80e2a81f4fba1fd39a57aa4c65fdda45` on `main`.
- **Baseline evidence:** 1,048 locked Rust tests; clippy and fmt passed; 94.25%
  aggregate region coverage with every gated file at least 80%; all four adapter
  suites passed. One independent review of that fix batch was resolved. These are
  code checks, not hosted enforcement or real harness compatibility evidence.
- **Current task:** P3, safe owned-install cleanup and legacy removal.
- **Scope decision (2026-09-14):** the user replaced mandatory hosted-repository
  and live-harness proof with Docker feature E2E tests. Adapter domains or separate
  projects own their respective harness evolution and qualification.
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

Ship the stable core/CLI with a local feature suite using fixture files and an
installed deterministic test driver. No hosted repository, model credentials, or
real AI runtime is needed. This proves CLI behavior and consumer integration;
it does not prove a tamper-resistant permission boundary or real event delivery.

Harness adapters and external acceptance integrations own their own compatibility
and deployment claims. They may live in their current domains or separate projects.
No repository extraction or deletion of existing adapters is part of the feature
test batch; keep their tests and captures until safe cleanup identifies retained
callers. Do not make a new harness release a core release prerequisite.

No service, broker, daemon, verdict cache, scheduler, or additional harness is in
scope. Freeze gate-bash, proposal simulation, telemetry, watch, and self-update
expansion. Remove obsolete docs instead of maintaining historical roadmaps.

## Work and dependencies

| ID | Work | Depends on | Status | Evidence |
| --- | --- | --- | --- | --- |
| P0 | Select local feature boundary | None | Done | Scope decision above |
| P1 | Docker feature E2E suite | P0 | Done | Nine feature groups passed; command below |
| P2 | Separate adapter/integration qualification from core | P0 | Done | Spec §§4, 9, 12; `docs/adapters/README.md` |
| P3 | Owned-install cleanup and removal of old execution paths | P1 validated | Pending | Not recorded |
| P4 | Integrated release verification and documentation | P3 | Pending | Not recorded |

P3 inventory can happen immediately; installed paths are removed only with a
tested cleanup procedure and truthful capability documentation. No live harness
or remote repository access blocks this work.

### P0: local reference setup

Use the existing Docker E2E pattern: build the Linux CLI from this checkout with
`Cargo.lock` and `--locked`; install a small shell test driver; run as an
unprivileged user with fresh home/config, policy, fixture files, and a local Git
repository. Disable runtime networking and mount no host home or Docker socket.

Use deterministic text and shell-syntax checks. Invoke the actual CLI, consent
flow, JSON result, and acceptance consumer. Stub only broken-evaluator cases.
Keep detailed evaluator edge cases in the fast Rust suites.

**Done:** `tests/e2e/features/README.md` names setup, run command, ownership, and
limits. The fixture driver's acceptance result identifies a detached commit checkout
after a complete pass. It is test machinery, not a production promotion service.

### P1: feature E2E suite

**Read:** spec §§5–8, 11, 14; existing CLI tests and `tests/e2e/features/`.

Exercise consent, red → feedback → repair → green, known/unknown paths, batches,
full acceptance after a write without feedback, committed candidate inputs,
policy selection/drift, missing executables, timeout/unrun results, and malformed
JSON. Reuse `scripts/verify-acceptance.sh` for complete-result consumption.

**Done:** `bash tests/e2e/features/run.sh` passes and runs as the `features` CI job.
The suite fails on mismatched exits, diagnostics, selected checks, or accepted
commit. Containers and run-specific image tags are cleaned; no live installs or
provider calls occur. Record actual validation below before closing P1.

### P2: adapter and integration ownership

Core owns config, events, exits, JSON, and bounded command execution. Adapter
owners translate real events to `change`, deliver diagnostics, and test their
runtime versions independently. Optional pinned-harness smoke tests belong there.
Preserve existing capture provenance and capture-pending declarations.

An integration advertising enforced acceptance still qualifies spec §4 against
its actual permissions, candidate binding, approved evaluator/policy, and result
publication. The core Docker suite cannot replace that qualification and does
not claim to. Neither qualification is a core release gate.

### P3: remove obsolete execution and installation paths

Trace callers in CLI, core, adapters, installer templates, tests, scripts, and docs
before deletion. Use existing ownership records to remove/replace only IronLint
registrations; show a concrete manual action where ownership is ambiguous.
Test chained/user-edited hooks and a repeated cleanup in a temporary home/repo.

Delete the Bash-gate crate/command, unversioned dispatch, schema-6 result consumers,
preview stdin/per-file ABI, `extends`/`steps`/suppression execution, unused diff
selection, and default floor installation when no retained caller needs them.
Update `init` to create v1 config. Keep harness installation and qualification
adapter-owned; do not promise automatic live support. Unsupported adapters must
not keep registrations calling deleted commands.

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

Record the tested commit or working-tree scope, toolchain/container setup, exact
command, and result here. Keep detailed logs in CI output, not a second planning
archive. Adapter captures stay in adapter-owned fixture directories.

| Required proof | Owner | Current evidence |
| --- | --- | --- |
| Editable red state, visible diagnostics, fixture rejects red and accepts green | P1 | Local Docker E2E passed |
| Nonzero command and missing/broken evaluator deny fixture acceptance | P1 | Local Docker E2E passed |
| Committed candidate differs from unstaged/staged repairs; new commits re-evaluated | P1 | Local Docker E2E passed |
| Explicit policy selection and renewed consent after policy change | P1 | Local Docker E2E passed |
| Write without feedback is caught by full acceptance | P1 | Local Docker E2E passed |
| Batch runs once per selected check; acceptance includes every check | P1/P4 | Core tests; local Docker E2E passed |
| Output caps, total deadline, pipe-holding descendant cleanup | P4 | Baseline core tests; rerun on release tree |
| Owned-entry removal preserves unrelated hooks and leaves no dead calls | P3 | Cleanup proof pending |

**Local feature evidence (2026-09-14):** working tree based on
`fc3936eee4da9d9becd31e485b82bac01edb3000`; `bash tests/e2e/features/run.sh`
passed all nine feature groups using Docker 29.4.0, Rust 1.88 Bookworm build,
Debian Bookworm runtime, and the installed fixture driver. No real AI harness or
hosted repository was used. `bash scripts/test-verify-acceptance.sh` and shell
syntax checks passed. Independent review found archive export omission and
multiple-JSON false-accept cases; both were reproduced with failing regressions
and fixed with detached checkout and a single-document verifier. The full Docker
suite passed again. Full Rust release validation remains P4 after P3 removal.

Run `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`,
`cargo fmt --all --check`, `bash scripts/ci-coverage.sh`, and
`bash scripts/ci-adapters.sh` while legacy suites are active, and
`bash tests/e2e/features/run.sh` against the integrated tree. Use a temporary
`XDG_CONFIG_HOME` for adapter tests; never exercise install/trust tests against the
developer's live configuration. Keep per-file region coverage at least 80%.

Request one separate review of each integrated implementation batch. Reopen a
resolved batch only for a newly reproducible regression. Final review covers
schema consumers, fixture scope, installation cleanup, and doc/code agreement.
Publishing is a separate action requiring authorization; do not infer it from a
green test suite. Do not declare v1 complete before local feature tests and
owned-install cleanup pass. Hosted enforcement and live harness proof are separate
integration-owned claims.

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
