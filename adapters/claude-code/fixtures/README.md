# claude-code contract fixtures (W4 — docs/architecture.md)

Pinned **live-captured** PreToolUse payloads, one per tool shape the adapter
reads. The contract suites (`crates/ironlint-cli/tests/hook_contract_claude_code.rs`)
load these as the happy-shape pin; embedded synthetic payloads remain for
malformed/adversarial edge cases.

## Schema

Every fixture is a JSON object with a mandatory `_provenance` header:

```json
{
  "_provenance": {
    "harness": "claude-code",
    "harness_version": "<exact `claude --version` output>",
    "captured_at": "2026-08-…T…Z",
    "captured_by": "capture.sh teeing PreToolUse stdin during a real `claude -p` session in a scratch repo"
  },
  "payload": { "...verbatim PreToolUse payload from stdin..." }
}
```

A fixture without a parseable provenance header fails the contract tests
(`assert_fixture_provenance`).

## Capture procedure (initial set + recapture after harness releases)

1. Scratch repo; `.claude/settings.json` with a `PreToolUse` hook that tees
   stdin to a file (`capture.sh`): `{"hooks":{"PreToolUse":[{"matcher":"Bash|Write|Edit","hooks":[{"type":"command","command":"<path>/capture.sh"}]}]}}`
2. One Write-like call (`claude -p "create a file…"`), one Bash-like call
   (`claude -p "list the files…"`), one Edit call.
3. Save each payload as `write.json` / `bash.json` / `edit.json` with the
   provenance header; remove instrumentation.
4. On a claude-code release touching hooks: run the `adapter-drift-audit`
   skill, recapture, bump `harness_version`.

## Pending

Fixtures not yet captured in this environment (no live claude-code session
available). Until `write.json`/`bash.json` exist, the contract suites fall
back to embedded synthetic payloads and print a loud capture-pending note.
