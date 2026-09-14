# V1 verdict JSON

Use `ironlint check --event accept --format json` with a `version: 1` policy.
V1 uses `schema: 7`. The current unversioned path has a different schema-6 output;
it is scheduled for removal and must not be used by v1 consumers.

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
| `schema` | Integer `7`; validate the expected schema exactly. |
| `event` | Normally `change` or `accept`; invalid input can report the invalid event. |
| `status` | `pass`, `violation`, `error`, or `not_run`. |
| `results` | Completed check results in check-ID order. |
| `not_run` | Selected checks left unrun, each with `id` and `reason`. |
| `error` | Nullable top-level diagnostic, including pre-execution failures. |

Each result has `id`, `outcome` (`pass`, `violation`, `error`), nullable numeric
`exit_status`, byte arrays `stdout`/`stderr`, stream truncation flags, and nullable
`reason`. Output is arrays of integers 0–255, preserving invalid UTF-8, not JSON
strings. Decode for display. Each stream is capped at 64 KiB; truncation does not
determine the outcome.

Errors or selected checks left unrun make the aggregate `error`, even if another
check violated policy. Otherwise a violation makes `violation`; nonempty all-pass
results make `pass`. An empty change selection is `not_run`, which may exit 0.

| CLI exit | Meaning |
| --- | --- |
| 0 | Successful evaluation, including `not_run` on an empty change selection. |
| 1 | Config/input or pre-execution verification error. |
| 2 | Violation, with no execution error taking precedence. |
| 3 | Execution error or incomplete evaluation. |
| 4 | Local policy lacks execution consent. |

Command exits 1–125 are violations. Exits 126/127, exits at least 128, signals,
spawn failures, and timeouts are execution errors. CLI codes are aggregate
results, not the raw check exit status.

An enforcing consumer requires exit 0, valid schema 7, event `accept`, status
`pass`, no error, no unrun checks, and exactly the expected required checks with
passing outcomes. It must bind the result to the candidate and approved
policy/evaluator. JSON alone is not an authorization receipt.

Source: `crates/ironlint-core/src/verdict.rs`; regression coverage:
`crates/ironlint-cli/tests/cli_v1.rs` and core verdict tests.
