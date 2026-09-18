# Running checks

For a policy with `version: 1`:

```sh
ironlint check                                      # all acceptance checks
ironlint check --event accept --format json
ironlint check --event change --file src/a.rs --file src/b.rs
ironlint check --event change                       # changed paths unknown
ironlint check --config /work/policy.yml --root /work/candidate --event accept
```

Commands run from `--root` (the current directory by default), with stdin
closed. A relative `.ironlint.yml` can be found in a parent directory up to the
Git boundary, but that does not change the root. Relative trigger paths resolve
under `--root`. Deleted paths are valid; escaping paths are errors.

`accept` runs every check. Repeatable `--file` is valid only with explicit
`--event change`. Omitting `--file` means changed paths are unknown. An
integration can provide a known-empty path set.

Use the separate read-only `ironlint explain` command to inspect selection.

See [JSON results](../reference/verdict-json.md) for exits and results. A change
invocation can return exit 0 with `not_run`; acceptance needs a complete pass.

Default execution limits are 30 seconds per check and 300 seconds per batch.
IronLint continues after violations, stops on execution errors or an exhausted
budget, and reports selected checks left unrun. Checks execute serially once
each, including for a multi-file batch.
