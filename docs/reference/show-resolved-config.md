# Resolved config output

```sh
ironlint show-resolved-config --config /work/policy.yml --format json
```

For a v1 policy with `tests: {run: 'exit 0'}`, JSON is:

```json
[
  {
    "check": "tests",
    "origin": "/work/policy.yml",
    "files": [],
    "run": "exit 0"
  }
]
```

Rows are in check-ID order. `origin` identifies the config path; v1 has no
inheritance. `files: []` means no trigger restriction. `run` is the command.
TSV (default) and YAML are also available; use `--help` for format choices.

This is a read-only inspection view, not a lossless policy export. It omits
`on`, version, and execution budgets. Use `explain` for change/acceptance
selection and read the source policy for complete configuration.
