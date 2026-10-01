# Pi controlled completion qualification — 2026-10-01

## Provenance

- Runtime: `@earendil-works/pi-coding-agent` 0.87.1 and `@earendil-works/pi-ai` 0.87.1, exact versions in `package-lock.json`.
- Upstream source: [`f07218c4d4bbc12bef056a7058c3dd49dfe41abe`](https://github.com/earendil-works/pi/tree/f07218c4d4bbc12bef056a7058c3dd49dfe41abe); [release](https://github.com/earendil-works/pi/releases/tag/v0.87.1), [SDK](https://github.com/earendil-works/pi/blob/f07218c4d4bbc12bef056a7058c3dd49dfe41abe/packages/coding-agent/docs/sdk.md), [extensions](https://github.com/earendil-works/pi/blob/f07218c4d4bbc12bef056a7058c3dd49dfe41abe/packages/coding-agent/docs/extensions.md).
- Host: macOS Darwin 25.6.0, arm64; Node v24.18.0; Pi CLI 0.87.1.
- Tested branch base: `20b3f8a213c670659105f24c3684ea273ccbc15c`; implementation commit: `212c427`.
- Capture commands: `pi --version`; `IRONLINT_TEST_BIN=/absolute/ironlint npm test` from `adapters/pi`; `bash scripts/test-verify-acceptance.sh`; `IRONLINT_TEST_BIN=/absolute/ironlint bash scripts/test-check-recipes.sh`.

## Capability audit

The `adapter-drift-audit` Pi source map is at
`.agents/skills/adapter-drift-audit/references/pi.md`. The pinned SDK source and
local installed 0.87.1 package agree on `agent_settled`, queued work,
`prompt`/`abort`/`waitForIdle`/`dispose`, custom tool registry precedence, and
`tool_result` extension feedback. The current installed release is 0.87.1.
No drift was found for the mapped contracts. The exact completion runner pins
0.87.1; the independently installed feedback extension retains its earlier
compatibility claim.

This qualifies **local workflow completion** for the explicit SDK runner under
the assumptions in [completion.md](completion.md). It does not qualify the v1
contract's tamper-resistant enforced acceptance across same-account adversaries.
The runner was tested on macOS; Linux CI is configured but has not yet run on
this branch. Windows cleanup and filesystem semantics are unqualified.

## Deterministic results

The real Pi 0.87.1 SDK with a scripted provider showed clean settlement, queued
follow-up before settlement, and abort that did not yield a clean task result.
The real SDK plus real IronLint CLI denied a violating candidate, accepted a
fresh repaired candidate, exhausted a zero repair budget, rejected an attempted
owner-policy write, and ran full acceptance despite no edit event and forged
model success text. Candidate unit scenarios cover partial staging, dirty and
untracked inputs, deletions, executable mode, declared ignored inputs, source
drift, artifact drift, and racing edits. Unsettled cancellation reports its
remaining uncertainty.

The shell and SDK acceptance consumers share
`tests/fixtures/acceptance-conformance.json`; truncated output, missing fields,
incomplete check sets, malformed output, and nonzero command statuses cannot
produce a passing decision. All nine real CLI recipe scenarios passed with a
temporary `XDG_CONFIG_HOME`.

Separate review found three issues in the integrated batch: a case-insensitive
`.GIT` path escape, missing-field disagreement between consumers, and cleanup
uncertainty after Pi abort grace. Each has regression coverage and a fix.

## Live model measurements

No live provider-backed comparison has been run. A bounded controlled smoke
attempt using the configured OpenRouter default stopped with a provider 401
(`API key expired`) before any model tool call or acceptance check. The local
Ollama endpoint was unavailable. Feedback-only versus controlled completion
rate, violations accepted, repair turns, check invocations, elapsed time, and
token cost therefore have **no denominator and no measured result**. The
fixed-task live suite required by H3 remains pending until a working model
credential or endpoint is available. Deterministic scripted provider runs
establish host behavior, not practical model repair utility.

## Integrated gates

| Gate | Result |
| --- | --- |
| `npm run typecheck` | Passed |
| `IRONLINT_TEST_BIN=... npm test` | Passed, 61/61 |
| `bash scripts/test-verify-acceptance.sh` | Passed |
| `IRONLINT_TEST_BIN=... bash scripts/test-check-recipes.sh` | Passed, 9/9 |
| `cargo test --locked --workspace --no-fail-fast --quiet` | Passed with `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=commit.gpgsign GIT_CONFIG_VALUE_0=false` to suppress the host's signing setting in fixture commits |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `bash scripts/ci-coverage.sh` | Passed; workspace regions 94.07%, every Rust source file at least 80% |
| `bash tests/e2e/features/run.sh` | Passed, local Linux feature E2E suite |
| `npm pack --dry-run --json` | Passed with a temporary npm cache; launcher and sources included |

The tested branch is not a release claim until H3 live measurement and the
remaining H5 evidence are complete. Core IronLint remains independently usable.
