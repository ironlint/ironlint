# Diagnostics

When a policy will not run or an optional adapter appears inactive, start with:

```sh
ironlint doctor
```

`doctor` is read-only. It checks the local binary, policy, shell, execution
consent, and adapter files it can see, then provides a remediation hint for each
problem. It exits 0 when every check passes or warns, and 1 when any check fails.

Use `--format json` for a machine-readable report.

## Checks

| Name | What it verifies |
| --- | --- |
| `binary` | The running `ironlint` resolves to a path and reports a version. |
| `config` | `<dir>/.ironlint.yml` exists. |
| `parses` | The v1 policy parses. |
| `check_scripts` | A single-path command under `.ironlint/` exists and is executable. |
| `shell` | A POSIX `sh` is on `PATH`. On Windows, use Git Bash or WSL. |
| `trust` | The policy and managed scripts have local execution consent. |
| `pi` | The optional Pi adapter is present and matches its installed files. |
| `hooks` | Summary of optional adapter installations. |

The adapter row can be omitted when neither Pi nor its IronLint adapter is
found. A modified or outdated optional adapter produces a warning. An installed
file is only an on-disk fact; no local diagnostic can prove every edit passes
through an adapter.

## Report shape

```json
{
  "ironlint_version": "<x.y.z>",
  "checks": [
    {
      "name": "config",
      "status": "pass",
      "detail": "/work/repo/.ironlint.yml exists",
      "remediation": null
    }
  ]
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `ironlint_version` | string | Version of the running binary. |
| `checks` | array | Checks in the order shown above. |
| `name` | string | Stable check ID. |
| `status` | `"pass"` \| `"warn"` \| `"fail"` | Outcome. Any failure makes the command exit 1. |
| `detail` | string | Human-readable observation, which can include paths or versions. |
| `remediation` | string \| null | Suggested next action, or `null` for a pass. |

Do not parse `detail` or `remediation`; they are written for people and can
change between releases.
