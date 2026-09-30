# IronLint — Pi adapter

This adapter provides feedback only. It does not control repository acceptance.

## Install

Create, validate, and trust the policy from [Getting started](../../docs/getting-started.md)
first. Then install the project-scoped extension:

```sh
ironlint init --harness pi
ironlint doctor
```

`init` leaves an existing `.ironlint.yml` in place. Pi 0.85.1 and `ironlint`
1.1.0 or newer must be available to the project. Use `ironlint init --uninstall --harness pi`
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

If a later successful mutation arrives while evaluation is running, it cancels
the older invocation, including when that mutation removes the policy. An
actively canceled callback appends no stale output. Already completed results
that race a newer edit are labelled superseded. Runs are owned by the extension;
it does not retry evaluation.

## Feedback limits

Each feedback invocation has a 600-second wall-time ceiling, an 8 MiB stdout
ceiling, 64 KiB retained stderr, and a 2-second termination/closure grace. These
protect the host and are independent of policy timeouts. Longer valid policies
can still run through the standalone CLI.

Wall-time expiry or excess stdout stops the run and reports incomplete feedback
with a reproduction command. Truncated stdout is never parsed as a complete
verdict. Excess stderr is drained and clipped with a truncation notice. Capture
counts bytes before UTF-8 decoding, including JSON's expansion of byte arrays.
An exit-zero response is silent only when it contains a valid complete schema-7
`change` result; malformed JSON or invalid UTF-8 stdout is an evaluation error.

Cleanup first closes a dedicated evaluator input channel. The evaluator stops
its active check through its existing command cleanup; checks still receive
closed stdin. If cooperative cleanup does not complete within the grace period,
the adapter forcibly stops its owned evaluator and its Unix process group, settles,
and reports unconfirmed cleanup. An older evaluator that rejects the cancellation
capability produces incomplete feedback. The reproduction command uses ordinary
standalone evaluation. Run timers and callbacks are cleared; constant error sinks
prevent late events from crashing the host. Process groups are not isolation,
and Windows descendant cleanup is not qualified by these fixtures.

## Limitations

The adapter only receives the tool results Pi sends to extensions. If Pi never
delivers a `tool_result`, there is no early feedback. Fixture tests prove callback
handling, not live host event delivery. Run a complete
`ironlint check --event accept` before treating work as ready; it catches changes
that did not receive post-edit feedback.
