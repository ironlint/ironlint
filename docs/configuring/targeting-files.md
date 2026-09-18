# Select checks after a change

`files` selects early `change` feedback. `accept` runs every check.

```yaml
version: 1
checks:
  format:
    files: ["*.rs", "Cargo.toml"]
    on: [change, accept]
    run: cargo fmt --all --check
```

Bare globs match at any depth: `*.rs` includes `src/lib.rs`. Globs containing
`/` are relative to the evaluated root. Without `files`, a check runs
unconditionally for its declared events. Every selected command runs once.

Pass known paths with repeatable `--file` and explicit `--event change`. Include
deletions and both rename endpoints. Paths need not exist, but must stay within
`--root`. In-root symlink names are preserved for trigger matching.

Omitting CLI `--file` means changed paths are unknown and selects all change
checks. An integration can also report a known-empty path set: file-filtered
checks stay unselected, while unconditional change checks run.

`files` is a trigger, not a dependency graph or command sandbox. Commands inspect
their own inputs. Use `ironlint explain PATH --root ROOT` to inspect change
selection.
