# JSON results

Use `ironlint check --event accept --format json` with a `version: 1` policy.
The JSON result uses `schema: 7`.

```json
{
  "schema": 7,
  "event": "accept",
  "status": "pass",
  "results": [{
    "id": "tests",
    "outcome": "pass",
    "exit_status": 0,
    "stdout": [],
    "stderr": [],
    "stdout_truncated": false,
    "stderr_truncated": false,
    "reason": null
  }],
  "not_run": [],
  "error": null
}
```

| Field | Meaning |
| --- | --- |
| `schema` | Integer `7`; consumers should validate it exactly. |
| `event` | `change` or `accept`. |
| `status` | `pass`, `violation`, `error`, or `not_run`. |
| `results` | Completed check results in check-ID order. |
| `not_run` | Selected checks left unrun, each with `id` and `reason`. |
| `error` | Nullable top-level diagnostic, including failures before execution. |

Each result has `id`, `outcome` (`pass`, `violation`, or `error`), nullable
numeric `exit_status`, byte arrays `stdout` and `stderr`, truncation flags, and
nullable `reason`. Output is arrays of integers 0–255, preserving invalid UTF-8
for callers that need exact bytes. Each stream is capped at 64 KiB.

Errors or selected checks left unrun make the aggregate `error`, even if another
check violated policy. Otherwise a violation makes `violation`; completed
all-pass results make `pass`. An empty change selection is `not_run` and can
still exit 0.

| CLI exit | Meaning |
| --- | --- |
| 0 | Evaluation completed, including empty `change` selection. |
| 1 | Config, input, or pre-execution verification error. |
| 2 | Policy violation, with no execution error taking precedence. |
| 3 | Execution error or incomplete evaluation. |
| 4 | The policy lacks local execution consent. |

Command exits 1–125 are policy violations. Exits 126/127, exits at least 128,
signals, spawn failures, and timeouts are execution errors. CLI codes describe
the whole evaluation, not a single command's exit status.

If another tool treats this result as approval, require an `accept` event,
`status: "pass"`, no errors or unrun checks, and the expected checks with
passing outcomes. It must also connect that result to the exact change it
evaluated.
