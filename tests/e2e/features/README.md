# Local feature end-to-end tests

Run from any directory:

```sh
bash tests/e2e/features/run.sh
```

Requires Docker. The image builds the real Linux CLI with `Cargo.lock` and
`--locked`, installs a tiny shell fixture harness, and runs as an unprivileged
user with a fresh home, consent store, policy, files, and Git repository. Runtime
networking is disabled. No host directories, credentials, Docker socket, remote
repository, real AI harness, or model provider are used. Build dependencies need
network access on the first build; later builds reuse Docker's cache. Containers
and the run's image tag are removed on completion.

## Features covered

- Trust before execution and renewed consent after policy changes.
- Real checks, visible diagnostics, editable red state, repair, and complete pass.
- Change filtering, unknown paths, duplicate/batch paths, and one run per check.
- Full acceptance catches mutations that received no early feedback.
- Acceptance checks a detached commit checkout, not a repaired or partially staged
  tree; archive export attributes cannot omit a violating file.
- New commits are evaluated again; candidate config does not select the policy.
- Timeouts and remaining unrun checks, missing commands/evaluator, malformed or
  multiple JSON verdicts.

The fixture's `accept` operation prints the evaluated commit only after the
existing acceptance verifier succeeds. This is a test consumer of the CLI,
not a production hook or permission boundary. The same user can modify the
fixture harness: these tests do not prove hostile-candidate isolation, hosted
branch protection, merge-result binding, or any real harness event contract.

Core owns `change`/`accept`, configuration, exits, schema-7 JSON, and process
bounds. Adapter owners maintain event translation, harness installation, and
their own optional pinned-runtime smoke tests. They may live in their adapter
domain or a separate project. Core releases require no live harness capture.

The existing `../init/` suite separately checks legacy installer artifacts using
seeded harness directories; it does not execute real harnesses either. Keep it
until owned-install cleanup retires the legacy paths. Detailed process limits,
path validation, and schema cases remain in the fast Rust test suites.
