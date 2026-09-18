# Get started

IronLint runs shell commands you choose for a project. Put those commands in a
policy, review them, and ask IronLint to run either the checks relevant to a
change or the full set.

## Install

Clone this repository with Rust 1.88 or newer installed:

```sh
git clone https://github.com/ironlint/ironlint.git
cd ironlint
cargo install --locked --path crates/ironlint-cli
ironlint --version
```

IronLint uses POSIX `sh` to run policy commands. On Windows, use Git Bash or
WSL.

## Create a policy

Create `.ironlint.yml` in the project root. This Rust example runs formatting
after Rust changes and runs the full test suite when work is ready:

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

Each named entry is a check. `run` is the shell command to run. Checks run for
`accept` by default; add `change` and `files` when a check should also give
quick feedback for matching paths. `files` controls when IronLint starts a
check, not which files the command reads.

## Review, approve, and run it

```sh
ironlint validate
ironlint trust
ironlint check --event change --file src/lib.rs
ironlint check --event accept --format json
```

`validate` checks the policy without running commands. `trust` records your
approval of the policy and scripts under `.ironlint/scripts/`. Run it again
after reviewing a change to either. Review other files called by a command too:
they are not part of the managed trust surface.

Use `change` when you know which paths changed. Use `accept` before you treat
work as complete: it runs every configured check once. A successful `change`
run can skip checks that do not match the supplied paths.

To evaluate a different policy against a different working tree, give both
paths explicitly:

```sh
ironlint check --config /work/policy.yml --root /work/candidate --event accept --format json
```

See [writing checks](writing-checks/README.md) for command behavior and
[execution consent](security/trust.md) for the security boundary. Optional
[AI-tool integrations](adapters/README.md) are separate from this direct CLI
workflow. `ironlint init` can scaffold this v1 policy and optionally install
the Pi feedback adapter.
