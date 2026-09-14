# IronLint

Run your project's checks with consistent selection, timeouts, and machine-readable
results. IronLint gives AI coding workflows a deterministic evaluation step using
the same shell commands you run locally or in CI.

**V1 is in progress.** The evaluator and CLI below work today. External acceptance
and one completed-edit feedback adapter are still to build. Current installers
and adapters use the older write-hook protocol and do not provide v1 integration.

```yaml
version: 1
checks:
  format:
    files: ["*.rs", "Cargo.toml"]
    on: [change, accept]
    run: cargo fmt --all --check
  tests:
    run: cargo test --locked
```

With this policy in `.ironlint.yml`, review its commands, then run:

```sh
ironlint validate
ironlint trust
ironlint check --event accept --format json
ironlint check --event change --file src/lib.rs --format json
```

Acceptance evaluation runs every check once. Change evaluation runs opted-in
checks whose trigger paths match. Commands inspect files on disk with stdin
closed. The caller consumes the results and decides whether to accept the
evaluated candidate. See [Getting started](docs/getting-started.md).

## Build the current source

From this checkout, with Rust installed:

```sh
cargo install --locked --path crates/ironlint-cli
ironlint --version
```

Commands require `sh`; on Windows use Git Bash or WSL. Published binaries may
predate v1. This documentation describes the current source checkout.

## Boundaries

IronLint runs arbitrary policy commands; it is not a sandbox. Local trust records
execution consent and detects managed policy/script changes. An external acceptance
integration must protect policy, evaluator, publication credentials, and the exact
revision it accepts. That integration is a release gate.

V1 is a breaking release. Backward compatibility, automatic config conversion,
and coordinated rollback are not planned. Removal of obsolete code and safe
cleanup of installed IronLint hooks are tracked in the active plan.

## Documentation and work

- [Documentation](docs/README.md): current usage and reference.
- [Architecture](docs/architecture.md): implemented flow, source map, and gaps.
- [V1 contract](specs/2026-09-05-ironlint-v1-design.md): required release behavior.
- [Implementation plan](plans/2026-09-05-ironlint-v1-implementation.md): next task and evidence.
- [Agent instructions](AGENTS.md): validation and development rules.
- [License](LICENSE): Apache 2.0.
