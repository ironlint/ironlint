# Current work

The [durability and simplification plan](2026-09-29-durability-simplification.md)
is the active engineering plan. It covers the 2026-09-29 review findings and the
per-check timeout extension, with ordered packets, regression requirements,
compatibility decisions, and a pressure test. Implementation is in progress with
sub-agents; the plan records current packet status and fresh verification.

The completed [v1 release plan](2026-09-05-ironlint-v1-implementation.md) retains
the verified release baseline. Tagging and publishing remain separate operator
actions; its completed packets are not reopened by the follow-up plan.

- [Current architecture](../docs/architecture.md): what the checkout implements.
- [V1 contract](../specs/2026-09-05-ironlint-v1-design.md): required release behavior.
- [Agent instructions](../AGENTS.md): working and validation rules.

Read the plan's resume block and assigned packet, then only the relevant contract
sections and source. Update status and evidence in place after each integrated
batch. A completed packet needs a tested commit and reproducible evidence; an
unchecked item is not implicitly done. Keep detailed transcripts out of the plan.

Historical plans and duplicate roadmaps are removed. Do not create archive copies
or carry abandoned requirements forward. Add a new plan only for separately
authorized work that does not fit the active v1 scope.

## Deferred

No additional work is queued. Add only deliberately requested ideas here, as one
dated line naming the work and when it would be worth doing. Deferred items do not
become release requirements without an explicit scope decision.
