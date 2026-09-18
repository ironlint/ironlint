# Inspect a policy

These commands do not execute checks or require execution consent:

```sh
ironlint validate --config /work/policy.yml --format json
ironlint explain src/lib.rs --config /work/policy.yml --root /work/candidate --format json
ironlint show-resolved-config --config /work/policy.yml --format json
```

`validate` reports whether the policy is valid. `explain` shows whether a path
triggers a check on `change` and confirms that every check remains required for
`accept`. Paths are validated relative to `--root`.

`show-resolved-config` lists checks, their policy paths, file triggers, and
commands. It is a compact inspection view, not a complete policy export. See
[output fields](../reference/show-resolved-config.md).

Use [doctor](diagnostics.md) to inspect the local shell, execution consent, and
optional adapter files.
