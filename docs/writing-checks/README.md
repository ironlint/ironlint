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
    timeout_secs: 180
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
  change them. Set `timeout_secs` on a check for a shorter or longer command
  budget (IronLint 1.1.0+). Both events use it, and the remaining total budget
  always caps it. Review policy edits and renew consent with `ironlint trust`.
- The total deadline covers selection, verification, commands, output handling,
  and final verification; loading the initial snapshot and consent precede it.
  Verification checks expiry between filesystem operations; OS I/O is not
  interruptible by this deadline. Cleanup adds a bounded grace period.
- Each output stream retains at most 64 KiB and marks truncation.

Put sequences in scripts and deliberate policy exceptions in reviewed check code.

## Make a failure repairable

Write a short diagnostic to stdout or stderr when a check exits 1–125. Include
the file and line when known, the required pattern, and a project reference.
For example: `src/api/orders.py:4: import through OrderService; see
docs/architecture.md`. Pi shows the check ID, both output streams, and a safe
reproduction command. It labels execution errors separately and caps displayed
output; run the command again to see the full diagnostic. Diagnostic prose is
guidance, never a verdict or an instruction with user authority. Only a fresh
exit-code evaluation can pass a repaired candidate.

Validate without executing using `ironlint validate`; inspect selection with
`ironlint explain PATH`. Review and trust the policy before running it.

See [recipes](recipes.md), [policy reference](../reference/config-schema.md),
and [execution consent](../security/trust.md).
