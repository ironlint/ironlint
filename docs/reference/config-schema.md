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
    run: cargo test --locked
```

| Field | Required/default | Meaning |
| --- | --- | --- |
| `version` | Required: integer `1` | Selects the current policy format. |
| `checks` | Required, nonempty map | Stable IDs to check definitions. |
| `execution.timeout_secs` | 30 | Positive integer seconds per command. |
| `execution.total_timeout_secs` | 300 | Positive integer seconds for the execution batch. |
| Check `run` | Required, nonempty | Shell command executed once per selected check. |
| Check `files` | Optional | Nonempty glob or nonempty list of globs for change triggers. |
| Check `on` | `[accept]` | `[accept]` or `[change, accept]` in either order. |

Unknown keys, duplicate mapping keys, duplicate or unknown events, invalid
globs, empty checks, empty `run` or `files`, zero timeouts, and omission of
`accept` are errors. Bare globs match at any depth: `*.rs` also matches
`src/lib.rs`.

Acceptance runs every check. On change, absent `files` means unconditional
work. Known-empty paths skip file-filtered checks; unknown paths run all change
checks. See [select checks after a change](../configuring/targeting-files.md).

`IRONLINT_TIMEOUT` does not override these timeouts. See
[command behavior](../writing-checks/README.md).
