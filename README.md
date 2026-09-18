# IronLint

IronLint runs the checks your project already trusts, so an AI coding workflow
gets the same clear pass or fail result as a local command or CI job. Define the
commands once in `.ironlint.yml`, then run them after a change or when work is
ready to review.

It is for developers who want project rules to be executable instead of buried
in prompts, review comments, or a checklist. IronLint does not invent rules or
change files: it runs the commands you choose and reports their output.

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

With that policy in the project root:

```sh
ironlint validate
ironlint trust
ironlint check --event change --file src/lib.rs
ironlint check --event accept --format json
```

`change` runs the fast checks that apply to known changed paths. `accept` runs
every check, which makes it the command to use before treating a change as
complete. Output is human-readable by default and available as JSON for tools.

## Install from source

To build the version in this repository, install Rust 1.88 or newer and a POSIX
`sh` (Git Bash or WSL on Windows).

```sh
git clone https://github.com/ironlint/ironlint.git
cd ironlint
cargo install --locked --path crates/ironlint-cli
ironlint --version
```

## What to read next

- [Get started](docs/getting-started.md): create and run your first policy.
- [Write checks](docs/writing-checks/README.md): choose commands and limits.
- [Run checks](docs/operating/running-checks.md): use `change` and `accept`.
- [Policy reference](docs/reference/config-schema.md) and [CLI reference](docs/reference/cli.md).
- [Execution consent](docs/security/trust.md): review policy commands before they run.
- [AI-tool integrations](docs/adapters/README.md): optional adapter-specific setup.

## Safety

A policy can run arbitrary shell commands with your account's permissions.
Review it before running `ironlint trust`; trust records your local approval and
is renewed when the policy or a script under `.ironlint/scripts/` changes. It
does not sandbox those commands, cover other files a command might invoke, or
replace protections in your hosting or CI system.

Licensed under [Apache 2.0](LICENSE).
