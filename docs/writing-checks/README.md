# Writing v1 checks

A check is a shell command over the actual project tree. IronLint selects and
runs it, captures diagnostics, and classifies its exit status.

```yaml
version: 1
checks:
  format:
    files: "*.rs"
    on: [change, accept]
    run: cargo fmt --all --check
  tests:
    run: cargo test --locked
```

Every check is required at acceptance. Adding `change` gives early feedback;
`files` narrows when that early run happens. Deletions and indirect changes
never remove an acceptance check. Each selected command runs once.

## Command contract

- `sh -c` executes `run` from the supplied root, with stdin closed.
- Inspect files on disk. Commands own file iteration and should check, not repair,
  candidate source. Build artifacts are allowed in designated paths.
- Exit 0 passes; 1–125 violates policy. Exits 126/127, high exits, signals, and
  timeouts are execution errors. Diagnostics do not change the outcome.
- Reserved variables: `IRONLINT_ROOT`, `IRONLINT_EVENT`, and `IRONLINT_BIN`.
  No per-file variables, proposed content, manifest, or proposal tempfile is supplied.
- Retained environment: `PATH`, `HOME`, `LANG`, `TZ`, `TMPDIR`, and `LC_*`.
  Other inherited variables are not forwarded. This is not a filesystem sandbox.
- Defaults: 30 seconds per check and 300 seconds total; configure `execution` to
  change them. Each output stream retains at most 64 KiB and marks truncation.

Put sequences in scripts and deliberate policy exceptions in reviewed check code.
V1 has no `steps`, inheritance, source suppression, or feedback-only rule type.

Validate without executing using `ironlint validate`; inspect selection with
`ironlint explain PATH`. Review and trust the policy before running it. An external
integration must separately approve policy/evaluator provenance; location inside
a candidate is not proof of approval.

See [recipes](recipes.md), [schema](../reference/config-schema.md), and
[consent](../security/trust.md).
