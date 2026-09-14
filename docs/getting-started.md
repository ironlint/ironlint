# Getting started

This walkthrough uses the implemented `version: 1` evaluator. Automatic feedback
and external acceptance integration remain pending. Use a source build from this
checkout; published binaries may implement an earlier format.

## Build

```sh
cargo install --locked --path crates/ironlint-cli
ironlint --version
```

A POSIX `sh` must be available. On Windows, use Git Bash or WSL.

## Write a policy

In your project, create `.ironlint.yml` if it does not exist. Review an existing
policy before replacing it. For a Rust workspace:

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

Every check participates in acceptance. `files` narrows early feedback only;
commands choose their inputs. They run against the actual tree with stdin closed.

## Validate and run

```sh
ironlint validate
ironlint trust
ironlint check --event accept --format json
ironlint check --event change --file src/lib.rs --format json
```

Run `trust` after reviewing the policy and managed scripts; they execute with
your account's filesystem permissions. Editing approved inputs requires renewed
consent. For a separate policy/root, use absolute paths explicitly:

```sh
ironlint check --config /work/policy.yml --root /work/candidate --event accept --format json
```

The root is the command's working directory. A pass means evaluation succeeded;
external acceptance requires a caller that protects and binds the same candidate.
JSON distinguishes `not_run` from `pass`.

## Current installation limitation

`ironlint init` still creates unversioned configs and installs the older
write-hook integrations. It does not complete v1 setup. Create a v1 policy
explicitly for now; [the plan](../plans/2026-09-05-ironlint-v1-implementation.md)
tracks installer replacement and safe removal of owned hooks.
