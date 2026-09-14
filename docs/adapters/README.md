# Adapter status

The four current adapters use the unversioned write-hook protocol. Their invocation
flags and verdict consumers do not implement v1 completed-edit feedback.

| Existing adapter | Installation details | Capture evidence |
| --- | --- | --- |
| Claude Code | [README](../../adapters/claude-code/README.md) | Pending; synthetic tests |
| Codex | [README](../../adapters/codex/README.md) | Pre-write apply_patch add/update |
| pi | [README](../../adapters/pi/README.md) | Pending; synthetic tests |
| OpenCode | [README](../../adapters/opencode/README.md) | Pending; synthetic tests |

`ironlint init` still creates unversioned config, installs these integrations,
and installs a Git floor hook. Existing uninstall/ownership machinery is the
starting point for cleanup. Preserve unrelated hooks, settings, and user edits
before deleting commands installed entries invoke.

## V1 adapter to build

Use a verified completed-edit/batch event. Translate known paths, including
deletions/renames, into `change` evaluation. Return visible diagnostics and a
reproduction command; keep edits. Suppress successes, bound synchronous execution,
and label observed superseded results. No proposal reconstruction or gate-bash.

Require real captures and visible red → repair → green proof. Pre-write captures
do not prove this behavior. V1 requires one live-verified feedback adapter;
additional harnesses can stay unsupported/capture-pending, without stale installed
calls to removed commands.

## Timeout budget

Existing write hooks run checks sequentially; a host timeout may expire first.
Consult each adapter README for current registrations. V1 feedback must fit the
configured total deadline and verified host contract. No detached work or retries.

## Contract fixtures

`bash scripts/ci-adapters.sh` runs all four existing suites. Use temporary
`XDG_CONFIG_HOME` locally. Preserve provenance and README capture-pending
declarations while suites remain active. Capture procedures live in each
`adapters/<harness>/fixtures/README.md`. Synthetic tests are not live proof.

See [architecture](../architecture.md) and [the plan](../../plans/2026-09-05-ironlint-v1-implementation.md).
