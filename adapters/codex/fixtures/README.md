# codex contract fixtures (W4 — docs/architecture.md)

Pinned **live-captured** PreToolUse payloads, one per tool shape the adapter
reads. The contract suites (`crates/ironlint-cli/tests/hook_contract_claude_code.rs`)
load these as the happy-shape pin; embedded synthetic payloads remain for
malformed/adversarial edge cases.

## Schema

Every fixture is a JSON object with a mandatory `_provenance` header:

```json
{
  "_provenance": {
    "harness": "codex",
    "harness_version": "<exact `codex --version` output>",
    "captured_at": "2026-08-…T…Z",
    "captured_by": "capture.sh teeing PreToolUse stdin during a real `codex exec` session in a scratch repo"
  },
  "payload": { "...verbatim PreToolUse payload from stdin..." }
}
```

A fixture without a parseable provenance header fails the contract tests
(`assert_fixture_provenance`).

## Capture procedure (initial set + recapture after harness releases)

1. Scratch repo; `.codex/hooks.json` with a `PreToolUse` hook that tees stdin
to a file (`capture.sh`): `{"PreToolUse":[{"matcher":"Bash|Write","hooks":[{"type":"command","command":"<path>/capture.sh"}]}]}`
2. One Write-like call (`codex exec "create a file…"`), one Bash-like call
   (`codex exec "list the files…"`), one Edit call.
3. Save each payload as `write.json` / `bash.json` / `edit.json` with the
   provenance header; remove instrumentation.
4. On a codex release touching hooks: run the `adapter-drift-audit`
   skill, recapture, bump `harness_version`.

## Pending

Fixtures not yet captured in this environment (no live codex session
available). Until `write.json`/`bash.json` exist, the contract suites fall
back to embedded synthetic payloads and print a loud capture-pending note.

## Captured fixtures (initial set)

`apply-patch-add.json` / `apply-patch-update.json` were captured **2026-07-03**
from `codex-cli 0.141.0` (model `gpt-5.5`) during a real interactive run (see
their `_provenance` headers; migrated from the legacy `tests/fixtures/codex/`
dir, which is now removed). Contract facts they lock: `tool_name` is
`apply_patch`; the patch lives in `tool_input.command` as a **bare envelope**
(`*** Begin Patch` … `*** End Patch`, no trailing newline, no heredoc wrap);
`*** End Patch` carries no trailing newline; Add File lines are `+`-prefixed;
Update File hunks may have an EMPTY `@@` header. Bash fixtures (`bash.json`)
are still capture-pending.
