# Current work

The [native completion enforcement plan](2026-10-02-native-completion-enforcement.md)
has completed N0–N3 for explicit Claude Code and Codex controllers, with pinned
macOS/Linux runtime evidence, bounded repair, and captured candidate acceptance.
Ordinary native sessions retain feedback hooks. The plan records exact support
limits; live model utility and package publication remain separate work.

The [coding-harness compliance plan](2026-10-01-harness-compliance.md) has an
integrated Pi completion/repair runner, actionable feedback, deterministic
real-runtime scenarios, and tested check examples. H3 live-model comparison and
the final H5 release record remain pending because the configured provider
credential expired. The core policy and verdict contracts remain unchanged.

The [durability and simplification plan](2026-09-29-durability-simplification.md)
is complete. It records the implemented review fixes and per-check timeout
extension, separate agent reviews, compatibility decisions, and fresh verification
as of 2026-09-30. No engineering packet remains open.

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

No additional deferred work is queued. Add only deliberately
requested ideas here, as one dated line naming the work and when it would be worth
doing. Deferred items do not become release requirements without an explicit
scope decision.
