# Policy reference

```yaml
version: 1
execution:
  timeout_secs: 30
  total_timeout_secs: 300
checks:
  format:
    files: ["*.rs", "Cargo.toml"]
    on: [change, accept]
    run: cargo fmt --all --check
  tests:
    timeout_secs: 180
    run: cargo test --locked
```

| Field | Required/default | Meaning |
| --- | --- | --- |
| `version` | Required: integer `1` | Selects the current policy format. |
| `checks` | Required, nonempty map | Stable IDs to check definitions. |
| `execution.timeout_secs` | 30 | Default positive integer seconds per command. |
| `execution.total_timeout_secs` | 300 | Positive integer seconds for the execution batch. |
| Check `run` | Required, nonempty | Shell command executed once per selected check. |
| Check `files` | Optional | Nonempty glob or nonempty list of globs for change triggers. |
| Check `on` | `[accept]` | `[accept]` or `[change, accept]` in either order. |
| Check `timeout_secs` | Inherits `execution.timeout_secs` | Positive integer command budget, for both events. |

Unknown keys, duplicate mapping keys, duplicate or unknown events, invalid
globs, empty checks, empty `run` or `files`, zero timeouts, and omission of
`accept` are errors. Bare globs match at any depth: `*.rs` also matches
`src/lib.rs`.

The command budget is the smaller of the check override (or global default) and
the remaining total budget. An override can exceed the default. Timeout values
must be integers greater than zero; null, strings, negative and fractional
values are invalid.

Check overrides require IronLint 1.1.0 or newer. Update the evaluator before
adding the field; older binaries reject it. Remove the field before downgrading.
Either edit changes the approved policy bytes, so review and renew consent with
`ironlint trust`. Existing policies and schema-7 verdicts retain their behavior.

The total deadline begins before selection and covers policy/script verification,
commands, output handling, and final verification. Initial snapshot loading and
consent lookup happen before it. Verification checks the deadline between file
operations and read chunks; OS I/O can overrun a cooperative deadline, and
process cleanup has a bounded grace period. A run whose final verification
expires returns an error even when all commands passed.

Acceptance runs every check. On change, absent `files` means unconditional
work. Known-empty paths skip file-filtered checks; unknown paths run all change
checks. See [select checks after a change](../configuring/targeting-files.md).

`IRONLINT_TIMEOUT` does not override these timeouts. See
[command behavior](../writing-checks/README.md).
