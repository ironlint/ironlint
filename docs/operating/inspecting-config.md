# Inspecting v1 config

These commands do not execute checks, write telemetry, or require execution consent.

```sh
ironlint validate --config /work/policy.yml --format json
ironlint explain src/lib.rs --config /work/policy.yml --root /work/candidate --format json
ironlint show-resolved-config --config /work/policy.yml --format json
```

For v1, `explain` reports each check's acceptance requirement and whether the
supplied path triggers change feedback. Acceptance remains required even when
the path does not match. Paths are validated relative to `--root`.

`show-resolved-config` lists checks, origins, file triggers, and commands; v1
has no inheritance. Its compact output is an inspection view, not a full policy
export: it omits event selection and execution budgets. See
[output fields](../reference/show-resolved-config.md).

Use [doctor](diagnostics.md) for shell, consent, config, and installed-artifact
diagnostics. Installation health does not prove external acceptance or live
completed-edit support.
