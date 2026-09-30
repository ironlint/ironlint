# CLI reference

The commands below use the policy format shown in [Getting started](../getting-started.md).

## Evaluate a policy

```text
ironlint check [--event accept|change] [--file PATH]...
               [--config PATH] [--root PATH] [--format human|json]
```

- The default event is `accept`, which runs every check.
- `--file` is repeatable and valid only with explicit `--event change`.
- The default root is the current directory. Relative trigger paths resolve under it.
- The default config is `.ironlint.yml`. A relative config path is searched in
  parent directories up to the Git boundary; it does not change `--root`.
- Human output is the default. It includes completed results, top-level errors,
  and every unexecuted check's ID and reason. Use `--format json` for
  machine-readable results.

Exit 0 means evaluation finished successfully, including an empty change
selection. Exits 1, 2, 3, and 4 mean input error, policy violation, execution
error, and missing execution consent. See [running checks](../operating/running-checks.md).

## Review a policy

```sh
ironlint trust --config /work/policy.yml
ironlint validate --config /work/policy.yml --format json
ironlint explain src/lib.rs --root /work/candidate --config /work/policy.yml --format json
ironlint show-resolved-config --config /work/policy.yml --format json
ironlint doctor --dir /work/project --format json
ironlint schema
```

`trust` records local permission to execute a policy. The other commands are
read-only and do not run checks. `explain` shows whether a path triggers a
check. `show-resolved-config` lists configured checks. `doctor` checks the
local setup. `schema` prints authoring help.

## Create a policy and install optional integration

```text
ironlint init [--dir DIR] [--harness pi] [--git-hook]
              [--yes] [--dry-run]
ironlint init --uninstall [--harness pi|all] [--dir DIR]
```

`init` creates a `version: 1` starter policy unless one already exists. The Pi
adapter is the only adapter available for new installation. `--git-hook`
explicitly adds a pre-commit hook that requires a complete trusted acceptance
pass; ordinary `init` does not change Git hooks. Uninstall removes owned files
and registrations while preserving edited or unrelated user content. See
[AI-tool integrations](../adapters/README.md).

The managed hook exits immediately on an IronLint failure. After a successful
acceptance check, any user-owned commands after the managed block still run.
Explicit installation activates an existing hook by adding owner execute
permission and preserving its other mode bits; new hooks use mode `0755` on Unix.
Doctor reports an owned hook that the current user cannot execute.

`update` downloads the installer to a temporary file, then runs it synchronously
for an installer-managed binary. Download failure prevents execution; installer
failure is reported. Download has a 60-second limit within a 300-second overall
deadline, followed by a bounded process cleanup grace. Installation is not
automatically retried. Temporary installers are removed on success or failure.
For a source installation, rebuild with Cargo.
