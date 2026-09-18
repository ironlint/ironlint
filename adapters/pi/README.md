# IronLint — Pi adapter

This adapter provides feedback only. It does not control repository acceptance.

## Install

Create, validate, and trust the policy from [Getting started](../../docs/getting-started.md)
first. Then install the project-scoped extension:

```sh
ironlint init --harness pi
ironlint doctor
```

`init` leaves an existing `.ironlint.yml` in place. Pi 0.85.1 and the `ironlint`
binary must be available to the project. Use `ironlint init --uninstall --harness pi`
to remove an unmodified extension; review an edited extension manually.

With Pi 0.85.1, it handles the documented `tool_result` event after a successful
`write` or `edit`. It runs `ironlint check --event change` synchronously,
then appends failures and evaluator errors to that tool result with a reproduction
command. The write remains in place and routine successful checks stay silent.

The adapter also translates custom completed `delete`, `unlink`, `rename`,
`move`, and `batch` tool-result shapes when they provide paths. A rename sends
both endpoints. A reported mutation with no path invokes change evaluation without
`--file`, which means unknown paths and runs all change checks.

Pi's built-in 0.85.1 tool set exposes `write` and `edit`; it has no native
delete, rename, or batch file tool. Mutations made through `bash` or other
unsupported tools receive no early feedback and require the full acceptance check.

If a later completed edit arrives while evaluation is running, the earlier feedback
is labelled superseded. The adapter waits for the CLI process; it does not retry or
start detached work.

## Limitations

The adapter only receives the tool results Pi sends to extensions. Run a complete
`ironlint check --event accept` before treating work as ready; it catches changes
that did not receive post-edit feedback.
