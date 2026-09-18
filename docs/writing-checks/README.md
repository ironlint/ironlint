# Writing checks

A check is a shell command that examines your project. IronLint selects and
runs it, captures its output, and reports whether it passed, found a policy
violation, or could not finish.

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

Every check runs at `accept`. Adding `change` gives early feedback; `files`
narrows when that early run happens. Deletions and indirect changes never remove
an acceptance check. Each selected command runs once.

## Command behavior

- `sh -c` executes `run` from the selected project root, with stdin closed.
- Inspect files on disk. Commands choose their own inputs and should check, not
  repair, source files.
- Exit 0 passes; 1–125 reports a policy violation. Exits 126/127, higher exits,
  signals, and timeouts are execution errors. Diagnostics do not change the outcome.
- Reserved variables: `IRONLINT_ROOT`, `IRONLINT_EVENT`, and `IRONLINT_BIN`.
- Retained environment: `PATH`, `HOME`, `LANG`, `TZ`, `TMPDIR`, and `LC_*`.
  Other inherited variables are not forwarded. This is not a filesystem sandbox.
- Defaults: 30 seconds per check and 300 seconds total; configure `execution` to
  change them. Each output stream retains at most 64 KiB and marks truncation.

Put sequences in scripts and deliberate policy exceptions in reviewed check code.

Validate without executing using `ironlint validate`; inspect selection with
`ironlint explain PATH`. Review and trust the policy before running it.

See [recipes](recipes.md), [policy reference](../reference/config-schema.md),
and [execution consent](../security/trust.md).
