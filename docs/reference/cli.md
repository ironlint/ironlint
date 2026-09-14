# CLI reference

This page covers the implemented v1 evaluator and identifies commands still tied
to the existing installation. The binary's help currently exposes both sets of
flags; a `version: 1` policy accepts only the v1 combinations below.

## Evaluation

```text
ironlint check [--event accept|change] [--file PATH]...
               [--config PATH] [--root PATH] [--format human|json]
```

- Default event: `accept`, which runs every check.
- `--file` is repeatable and valid only with explicit `--event change`.
- Default root: current directory. Relative trigger paths resolve under it.
- Default config: `.ironlint.yml`; use an absolute path to make discovery explicit.
- Default output: human. JSON uses [schema 7](verdict-json.md).
- `--diff`, `--content`, `--check`, `--force`, `--require-match`, and inline
  `--explain` are rejected for v1. `--allow-external-paths` does not relax v1 containment.

Exit 0 means successful evaluation, including empty change selection. Exits
1/2/3/4 mean input error, violation, execution error, and missing consent.
See [running checks](../operating/running-checks.md).

## Consent and inspection

```sh
ironlint trust --config /work/policy.yml
ironlint validate --config /work/policy.yml --format json
ironlint explain src/lib.rs --root /work/candidate --config /work/policy.yml --format json
ironlint show-resolved-config --config /work/policy.yml --format json
ironlint doctor --dir /work/project --format json
ironlint schema
```

`trust` records consent; inspect policy before using it. The other commands are
read-only and never run checks. `explain` reports acceptance requirement and
change selection. `show-resolved-config` supports TSV (default), YAML, and JSON.
`doctor` understands v1 config but reports the current installed adapter wiring;
that is not proof of v1 integration. `schema` currently prints a guide covering
both formats, pending installer/authoring cleanup.

## Existing installation commands

`init` still scaffolds unversioned config, grants consent to newly scaffolded
config, installs write-hook adapters, and installs the Git pre-commit floor.
It is not a v1 setup command yet. Run `ironlint init --help` for its current
scoping, dry-run, installation, and owned-entry removal options. Read the
[adapter status](../adapters/README.md) before changing installations.

`gate-bash` is a config-less shell classifier with exit 0 allow / 2 block.
Existing adapters fail closed on its errors. It is slated for removal with its
owned registrations; it does not establish external acceptance authority.

`watch --dir DIR` reads the existing write/pre-commit telemetry log. V1 does not
currently append those records. See [watch](../operating/watching-checks.md).

`update` reruns the release installer for an installer-managed binary; source
builds need rebuilding. Published releases may predate this checkout's v1 code.
Self-update expansion is outside the v1 plan.
