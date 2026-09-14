# Running v1 checks

For a `version: 1` policy:

```sh
ironlint check                                      # all acceptance checks
ironlint check --event accept --format json
ironlint check --event change --file src/a.rs --file src/b.rs
ironlint check --event change                       # changed paths unknown
ironlint check --config /work/policy.yml --root /work/candidate --event accept
```

Commands run from `--root` (current directory by default), with stdin closed.
Config discovery does not choose the candidate root for you. Relative trigger
paths resolve under this root. Deleted paths are valid; escaping paths are errors.

`accept` runs every check. Repeatable `--file` is only valid with explicit
`--event change`. Omitting `--file` means unknown changes. Only the core API can
receive a known-empty path set.

V1 rejects `--diff`, `--content`, `--check`, `--force`, `--require-match`, and
inline `--explain`. Use the separate read-only `ironlint explain` command.
There is no acceptance filtering or force-pass flag.

See [Verdict JSON](../reference/verdict-json.md) for exits and results. A change
invocation can return exit 0 with `not_run`; acceptance needs a complete pass.
`IRONLINT_FAIL_CLOSED_ON_INTERNAL` and `IRONLINT_TIMEOUT` do not change v1 semantics.

Default execution limits are 30 seconds per check and 300 seconds per batch.
The runner continues after violations, stops on execution errors or an exhausted
budget, and reports remaining selected checks as unrun. Checks execute serially
once each, including for a multi-file batch.
