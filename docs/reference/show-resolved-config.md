# Resolved policy output

```sh
ironlint show-resolved-config --config /work/policy.yml --format json
```

For a policy with `tests: {run: 'exit 0'}`, JSON is:

```json
[
  {
    "check": "tests",
    "origin": "/work/policy.yml",
    "files": [],
    "run": "exit 0",
    "timeout_secs": null,
    "effective_timeout_secs": 30
  }
]
```

Rows are in check-ID order. `origin` identifies the policy path. `files: []`
means the check has no trigger restriction. `run` is the command. TSV (default)
and YAML are also available; use `--help` for format choices.

`timeout_secs` is the explicit check override, or null when inherited.
`effective_timeout_secs` is that override or the global command default, before
the remaining total budget caps execution. For example, an override of 180
reports 180 even if the invocation has only 20 seconds left at runtime.
TSV appends override (empty when inherited) and effective seconds to the
existing check, origin, files, and run columns, for six columns total.

This is a read-only inspection view, not a lossless policy export. It omits
`on`, `version`, and the global execution block. Use `explain` for change and acceptance
selection, and read the policy for complete configuration.
