# Floor-hook spec completion: pre-filter removal, env-injection gate, captures, ship

**Date:** 2026-08-17
**Spec:** `specs/2026-08-17-git-floor-hook-and-self-defense-design.md` (W1–W4)
**Status:** Ready for execution

## Context for the executing session

W1 (git floor hook), W2 (placement heuristic + dogfood), W3 (gate-bash
self-defense blocklist), and W4 scaffolding (fixture dirs, provenance
meta-tests, `scripts/ci-adapters.sh`, CI lane) are **implemented and
uncommitted** in the working tree. Two review passes landed fixes; the
classifier was empirically re-probed and all bypass/over-block findings are
fixed and green. All workspace tests pass; clippy clean; `scripts/
ci-coverage.sh` passes (94.4% regions).

**What remains is a short punch list, ordered below. Tasks 1–3 are blocking
and must land before the commit in Task 5.**

## Task 1 — Remove the Bash pre-filter from all four adapters (BLOCKING)

All four adapters short-circuit Bash calls whose command text lacks
`ironlint`/`.ironlint`, which means none of the W3 surface (git bypass forms,
harness install-surface writes, `.git/hooks` protection) ever reaches
gate-bash through an adapter. Verified locations:

- `adapters/claude-code/hooks/hook.sh:85-91` (the `if [[ "${COMMAND}" != *ironlint* && != *.ironlint* ]] → exit 0` block)
- `adapters/codex/hooks/hook.sh:79-83` (same)
- `adapters/pi/src/index.ts:~227-235` (same substring predicate)
- `adapters/opencode/src/index.ts:~92-100` (same substring predicate)

- [ ] Delete the substring predicate in all four shims so every Bash tool
      call pipes the command to `ironlint gate-bash`. Keep each adapter's
      existing exit-code translation and fail-closed-on-broken-gate behavior
      exactly as-is.
- [ ] Measured cost justification (for the commit message): ~5.6ms per
      gated call with a release binary (100 invocations = 0.92s). Do NOT
      widen the substring list instead — that re-creates per-shim keyword
      drift, the disease this design kills.
- [ ] Tests, per adapter, driving the real shim/plugin (not the subcommand):
      a Bash tool call carrying `git commit --no-verify -m x` must block;
      `ls -la` must allow. Add to `tests/hook_contract_claude_code.rs`,
      `tests/hook_contract_codex.rs`,
      `adapters/pi/test/index.test.ts`,
      `adapters/opencode/tests/plugin.test.ts`.
- [ ] Check claims that Bash is gated "via gate-bash" in AGENTS.md and the
      drift-audit skill — after this task they become fully true.

## Task 2 — Gate `GIT_CONFIG_*` env injection in gate-bash

`env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/tmp/x git commit -m x`
is a documented-feature equivalent of `git -c core.hooksPath=…` and currently
passes the classifier (verified live, exit 0).

- [ ] Block when a segment case-insensitively contains both `GIT_CONFIG_`
      assignment(s) and `core.hookspath` (also cover `GIT_CONFIG_PARAMETERS`
      which embeds `'core.hooksPath=…'`). Add `assert_blocks` cases for all
      three forms plus allow-pins for innocent `GIT_CONFIG_GLOBAL=/dev/null
      git status`-style uses — actually evaluate that last one: if it
      deserves blocking too, pin the block instead.
- [ ] If any form is deferred, record it in the known-gaps paragraph in
      `lib.rs`'s module docs alongside the var-substitution gap.

## Task 3 — Make empty fixture dirs a hard failure in CI (W4 completion)

Today the fixture meta-tests warn-and-pass when `adapters/<h>/fixtures/`
holds only a README (true for claude-code, pi, opencode). Green-forever-on-
empty is the exact failure the fixtures defend against.

- [ ] In the four meta-tests (`tests/common/mod.rs` fixture helpers /
      `hook_contract_codex.rs` pattern, `adapters/pi/test/index.test.ts`,
      `adapters/opencode/tests/plugin.test.ts`): empty non-README dir →
      hard failure when `CI=true` (or `CI` unset-but-`--ci` arg), warn
      locally.
- [ ] Confirm the codex meta-test (already requires non-empty) keeps passing.

## Task 4 — Close the spec's open questions in-file

Edit `specs/2026-08-17-git-floor-hook-and-self-defense-design.md`: convert
"Open questions" into "Decisions":

- [ ] Q1 → decided: bare `--diff` dispatches per-check lifecycle
      (`DiffDispatch::Implicit` in `commands/check.rs`, pinned by
      `tests/cli_diff_lifecycle.rs`).
- [ ] Q2 → W3-R3 git-bypass forms shipped (sign-off obtained implicitly;
      user confirm at commit time).
- [ ] Q3 → check renamed to `rustfmt`.
- [ ] Q4 → bun + node on the adapters CI lane, confirmed.

## Task 5 — Trust, full gates, commit

- [ ] `./target/release/ironlint trust` — REQUIRED: `.ironlint.yml` changed
      (rustfmt-on-write → `rustfmt`, `on: [pre-commit]`); the hash is stale
      and the user's own floor will block commits until this runs.
      (`cargo build --release` first if the tree moved.)
- [ ] Full gates: `cargo test` → all green; `cargo clippy --all-targets --
      -D warnings`; `bash scripts/ci-coverage.sh` → all files ≥80% regions;
      `bash scripts/ci-adapters.sh` (needs node + bun).
- [ ] Commit the whole changeset in one commit or a small logical series
      (classifier / floor hook / fixtures+CI / docs). Suggested message
      includes the pre-filter removal rationale + the 5.6ms measurement.

## Task 6 — Post-commit smoke (floor goes live)

- [ ] `./target/release/ironlint init` in this repo → installs the floor
      hook into `.git/hooks/pre-commit` (verify marker block present,
      executable).
- [ ] Make a throwaway staged change + commit; confirm the floor fires
      (records in `.ironlint/log.jsonl` with the pre-commit batch for the
      `rustfmt` and `rust-pre-commit` checks).
- [ ] Deliberately stage a violation once (e.g. unformatted `.rs`) → commit
      must block with the check's message. Revert the throwaway.

## Task 7 — Backlog entries (use the `add-to-backlog` skill)

- [ ] Harness-watch weekly CI job: per-harness latest-version vs fixture
      provenance stamp + docs-hash watch → auto-file drift issues feeding
      the `adapter-drift-audit` skill.
- [ ] W5: `ironlint gate-tool --harness <id>` — dialect parse/response into
      the binary, shims collapse to stdin→binary→stdout wiring (deletes jq
      for shell hooks, shrinks pi/opencode TS).
- [ ] Live-capture contract tier: headless harness run in CI regenerating
      fixtures → auto-PR (deferred from W4 deliberately).

## Acceptance probes (regression reference)

The classifier battery in `crates/ironlint-bash-gate/src/lib.rs` already
pins these — re-verify empirically after Task 1–2 with a release binary:

Block: `git commit --no-verify -m x` · `git commit -n -m x` ·
`git -c user.name=x commit -n -m x` · `git -C /tmp commit -n -m x` ·
`git commit -qn -m x` · `git config core.HooksPath /tmp/x` ·
`git config core.hooksPath /tmp/x` · `git -c core.hooksPath=/t commit` ·
`rm -rf .git/hooks` · `rm -rf ~/.claude` · `cat > ~/.claude/settings.json` ·
(Task 2) the `GIT_CONFIG_*` forms.

Allow: `git log -n 5` · `git merge -n` · `git cherry-pick -n abc` ·
`git config --get core.hooksPath` · `echo hi > .claude/agents/foo.md` ·
`cp x sub/.claude/agents/` · `ironlint check` · `cp .codex/hooks.json
/tmp/backup` (source-read) · `ls -la`.

## Out of scope (do not build here)

`ironlint verify`; any watch-TUI work; autofix checks; telemetry schema
changes; AGENTS.md restructuring (already done — only touch the Bash-gating
claims if Task 1 changes their truth).
