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
| `pi` | The optional Pi adapter and ownership record are intact in each visible scope. |
| `claude-code`, `codex`, `opencode` | Legacy registrations or artifacts that require ownership-aware cleanup. |
| `git_hook` | An existing optional Git hook's managed block, ownership markers, and command form. |
| `hooks` | Summary of physical adapter artifacts and healthy installations. |

Adapter rows can be omitted when neither a harness nor an IronLint registration
or ownership record is found. Local and global installations are inspected;
scope and paths appear in `detail`. The same physical installation referenced
by both scopes may share a row. Independent settings or ownership records retain
separate rows, even when they reference one artifact. The summary counts a shared
physical artifact once and explicitly labels its healthy count.

Legacy registrations are unsupported; their remediation uses
`ironlint init --uninstall --harness <name>` and `--global` for global cleanup.
Missing owned artifacts, incomplete recovery records, and unreadable or malformed
settings are failures with the affected path. Modified or outdated optional
adapters produce warnings and preserve user content during cleanup.

The Git hook is inspected without executing it. Foreign content is preserved;
obsolete managed commands receive an update hint, and ambiguous ownership markers
are failures. An installed file is an on-disk observation; diagnostics cannot
prove every edit passes through an adapter.

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
| `name` | string | Check kind; adapter names can repeat for independent scopes. |
| `status` | `"pass"` \| `"warn"` \| `"fail"` | Outcome. Any failure makes the command exit 1. |
| `detail` | string | Human-readable observation, which can include paths or versions. |
| `remediation` | string \| null | Suggested next action, or `null` for a pass. |

Do not parse `detail` or `remediation`; they are written for people and can
change between releases.
