# Gate-bash + floor-hook adversarial-review fixes

**Date:** 2026-08-28
**Severity:** security hardening (rail-bypass closure)
**Upstream:** adversarial review of `2d088ac..3b6c3d3` (W1–W4 changeset). Every
finding was verified live — classifier payloads run through
`./target/release/ironlint gate-bash`, git/shell semantics proven in scratch
repos with a sentinel pre-commit hook.
**Status:** Ready for execution — two maintainer decisions pending (D1, D2 below).
**Sequencing:** L1→L5→L6 are sequential (same file, later rules match on top of
the earlier normalization fixes); L2/L3/L4 are file-disjoint and parallelizable
with L1; L7 last.

## Orchestration model (how this plan is meant to run)

Subagent-per-lane, model `deepseek/deepseek-v4-flash` (verified resolving +
responding in this environment 2026-08-28). If the `umans` provider gets
installed mid-flight, re-resolve the exact registry id (`subagent
action:"models"` → probe child) and swap the model string; nothing else
changes. Model is small: **zero design latitude** — every task below carries
the exact payloads, the exact reproduction command, and the fix shape. Any
deviation from the spec'd fix = escalate, don't improvise.

- One `workflowScript`, `worktree: true` per child (file-disjoint lanes in
  parallel; same-file lanes sequential).
- Every child runs under a host `gate` command (below). TDD per repo rules:
  pin the failing `assert_blocks`/`assert_allows` (or E2E) FIRST, watch red,
  fix, watch green.
- After all lanes merge: one adversarial-review pass over the cumulative diff
  by a separate reviewer agent (repo process rule), then the orchestrator
  re-runs the full probe battery (Appendix) against the final binary.

Gates:

- `GATE_CLASSIFIER` = `cargo test -p ironlint-bash-gate && cargo clippy -p ironlint-bash-gate --all-targets -- -D warnings`
- `GATE_HOOK` = `cargo test -p ironlint-cli --test cli_init_git_hook && cargo clippy -p ironlint-cli --all-targets -- -D warnings`
- `GATE_ADAPTERS` = `bash scripts/ci-adapters.sh`
- `GATE_FULL` = `cargo test && cargo clippy --all-targets -- -D warnings && bash scripts/ci-coverage.sh && bash scripts/ci-adapters.sh`

## Maintainer decisions (blocking — do not start L2 without answers)

**D1 — floor chaining (finding #1).** Recommended: **insert the marker block
BEFORE pre-existing hook content** (floor-first). A blocked commit runs nothing
else anyway, so the old "existing hook fails → floor must not run" precedence
concern is moot; floor-first preserves the blocking semantics of both hooks and
makes an `exit 0`/`exec` in user content harmless to the floor. Consequence:
our `trap` registers before theirs, so a pre-existing `trap … EXIT` clobbers
ours — mitigate by `rm -f "$diff_file"` explicitly before every exit in the
block (trap becomes backstop only). Spec W1-R3's append table gets rewritten.
Alternative (weaker): keep append + install-time detection of top-level
`exit`/`exec` in pre-existing content with a loud warning.

**D2 — config-missing guard (finding #7).** Recommended: **delete the
`[ -f .ironlint.yml ] || exit 0` line** — restores spec W1-R4's locked "exit 1
blocks" fail-closed posture. Accepted wedge: a commit that *deletes*
`.ironlint.yml` blocks until the human passes `--no-verify` deliberately
(spec's stated intent). Alternative (weaker): keep guard + print a loud stderr
warning + amend W1-R4 and AGENTS.md.

## Task L1 — classifier normalization core (`crates/ironlint-bash-gate/src/lib.rs`)

Foundation: every path/binary matcher downstream consumes these. Gate: `GATE_CLASSIFIER`.

- [ ] **L1.1 lexical path normalization.** Add `fn normalize_path_token(&str) -> String`
  (pure string op, no FS): collapse runs of `/`, resolve `.` and `..` segments
  lexically (pop on `..`, drop `.`), preserve leading `/`/`~`. Apply at the top
  of `is_policy_path`, `is_adapter_surface_file`, `is_adapter_surface_exec_dir`,
  `is_adapter_surface_profile_dir`, `is_floor_hook_path`, and the
  `.git/hooks` dir check. Failing pins first (all currently ALLOW, must BLOCK):
  - `echo x > .claude//settings.json`
  - `echo x > .claude/./settings.json`
  - `echo x > .git/hooks/./pre-commit`
  - `echo x > .git/hooks/../hooks/pre-commit`
  - `rm .git/./hooks/pre-commit`
  - `cp evil.sh .pi/./extensions/`
  - Allow-pins that must stay green: `echo hi > .claude/agents/foo.md`,
    `cp .codex/hooks.json /tmp/backup`, `ls sub/../src`.
- [ ] **L1.2 backslash de-obfuscation in `normalize`.** Shell unescapes `\x`→`x`
  outside quotes; `normalize` strips quotes but not backslashes. Strip a
  backslash that precedes any char (keep the char). Failing pins:
  - `echo x > .ironlint.ym\l` (verified: writes `.ironlint.yml`) → BLOCK
  - `ir\onlint trust` → BLOCK
  - Allow-pin: `echo "100\%"` stays allowed.
- [ ] **L1.3 empty/blank `core.hooksPath` value.** Root cause: quote-stripping +
  `split_whitespace` deletes the value token, making `git config
  core.hooksPath ''` textually identical to the sanctioned bare read
  (`git config core.hooksPath` stays ALLOW — pinned). Fix at the *raw segment*
  level, before normalization: in `config_mutates_hooks_path`, also block when
  the raw segment matches `core.hookspath` (case-insensitive) followed by
  whitespace + a quoted empty/whitespace-only string (`''`, `""`, `' '`, …).
  Failing pins (verified both disable hooks in real git):
  - `git config core.hooksPath ''` → BLOCK
  - `git config core.hooksPath ' '` → BLOCK
  - Allow-pin unchanged: `git config core.hooksPath`, `git config --get core.hooksPath`.

## Task L2 — floor hook (`crates/ironlint-cli/src/commands/init/git_hook.rs`, `tests/cli_init_git_hook.rs`)

Gate: `GATE_HOOK`. **Requires D1 + D2 answers.** Fix shape below assumes the
recommended options; adjust to the maintainer's call.

- [ ] **L2.1 floor-first install (D1).** `install`: when existing hook has no
  marker, write shebang + marker block + then the pre-existing content
  (de-duplicating a pre-existing shebang line). `replace_marker` / `uninstall`
  already position-agnostic; extend `is_deletable` usage unchanged. New E2E
  pins:
  - pre-existing `#!/bin/sh\nexit 0` hook + staged violation → commit BLOCKED
    (this is the exact scenario verified broken in review).
  - pre-existing sentinel-writing hook (falls through) + passing change →
    sentinel written AND floor ran (both execute, floor first).
  - pre-existing FAILING hook → commit blocked by user's hook (their verdict
    stands; floor already ran, harmless).
  - byte-preservation of pre-existing content on uninstall still holds.
- [ ] **L2.2 temp-file cleanup hardening.** In `hook_block`: explicit
  `rm -f "$diff_file"` before every `exit` in the marker block; keep the trap
  as backstop (a pre-existing `trap … EXIT` now clobbers OURS under
  floor-first — documented in the block comment).
- [ ] **L2.3 config-missing guard removal (D2).** Delete the
  `[ -f .ironlint.yml ] || exit 0` lines + their comment from `hook_block`.
  New E2E pin: config deleted from worktree → commit BLOCKED with the exit-1
  path (config/load error). Update the `hook_block_pins_exit_mapping_contract`
  unit test.
- [ ] **L2.4 spec sync.** Rewrite spec W1-R3's table to floor-first semantics;
  W1-R4 gains the "deletion commit wedges until `--no-verify`" note (or the
  D2 alternative text). Fix the live hook on this machine by re-running
  `ironlint init` after merge (recorded in L8).

## Task L3 — adapter Bash-branch hoist (`adapters/claude-code/hooks/hook.sh`, `adapters/codex/hooks/hook.sh`)

Gate: `GATE_ADAPTERS`. Finding #15.

- [ ] **L3.1** In both shell hooks, move the `TOOL_NAME == "Bash"` branch
  (gate-bash invocation + exit-code translation, byte-identical logic) ABOVE
  the `[[ ! -f "${CONFIG}" ]]` early-exit, mirroring pi/opencode's
  deliberate ordering ("most motivated when config is absent"). Config check
  then gates only the file-edit path.
- [ ] **L3.2** New contract-test pins in `tests/hook_contract_claude_code.rs` +
  `hook_contract_codex.rs`: in a fixture project with NO `.ironlint.yml`,
  a Bash payload carrying the `--no-verify` form (see Appendix) → deny;
  `ls -la` → allow. (pi/opencode already pin this shape.)

## Task L4 — fixture infrastructure (`crates/ironlint-cli/tests/common/fixtures.rs`, TS suites)

Gate: `GATE_ADAPTERS`. Findings #16, #17.

- [ ] **L4.1** Drop the dead `std::env::args().any(|a| a == "--ci")` branch
  from `is_ci` (libtest rejects the flag: verified `error: Unrecognized
  option: 'ci'`). Replace with `IRONLINT_TEST_CI=1` env; update the doc
  comment. Mirror in the two TS suites if they reference the arg.
- [ ] **L4.2** TS suites drive fixtures through the handler: substitute
  `__CWD__` with the fixture project dir, invoke the extension/plugin with
  `{toolName: "write"|"bash", input: <payload>}`, assert the allow/block
  result matches the fixture's intent (write.json → gate runs; bash.json
  `cat` → allow). opencode's dir is README-only — ship the code path so it
  activates on capture. Update `adapters/pi/fixtures/README.md` to record
  that pi is CAPTURED (3 fixtures, 0.84.3, 2026-08-27).

## Task L5 — classifier git rules (`crates/ironlint-bash-gate/src/lib.rs`)

Gate: `GATE_CLASSIFIER`. **Runs after L1 merges.** Findings #2–#5, #8.

- [ ] **L5.1 `--no-verify` abbreviation (finding #2).** Block any token that is
  a prefix of `--no-verify` of length ≥ 9 (`--no-veri`, `--no-verif`,
  `--no-verify`); `--no-ver` and shorter are ambiguous with `--no-verbose`
  (git rejects them) — leave alone. Subcommand-independent, same as today.
  Failing pin: `git commit --no-veri -m x` (verified: abbreviation accepted
  by git, hooks skipped, currently ALLOW).
- [ ] **L5.2 value-taking global flags (finding #3).** `git_subcommand`: add
  `--git-dir`, `--work-tree`, `--namespace`, `--config-env` to the
  skip-flag-and-value set (glued `--flag=value` forms already skip correctly
  via the `starts_with('-')` arm). Failing pins:
  - `git --work-tree /tmp commit -n -m x` → BLOCK (verified real bypass)
  - `git --git-dir /tmp commit -n -m x` → BLOCK
  - `git --namespace ns commit -n -m x` → BLOCK
  - Allow-pin: `git -c user.name=x commit -m x` (no `-n`) stays allowed.
- [ ] **L5.3 `--config-env` hooksPath injection (finding #4).** Block when a
  `--config-env` value (spaced or glued) names `core.hookspath`
  case-insensitively. Failing pin: `git --config-env core.hooksPath=MYV
  commit -m x` (verified real bypass) → BLOCK.
- [ ] **L5.4 path-prefixed git (finding #5).** Extract `is_git_binary(token)`
  = `token == "git" || token.ends_with("/git")`; use in
  `is_git_commit_escape` + `is_git_config_env_escape` (mirrors
  `is_ironlint_binary`). Failing pins:
  - `/usr/bin/git commit --no-verify -m x` → BLOCK (verified real bypass)
  - `GIT_CONFIG_KEY_0=core.hooksPath /usr/bin/git commit -m x` → BLOCK
- [ ] **L5.5 wrapper inventory (finding #8).** `strip_wrappers`: add `sudo`,
  `doas`, `command`, `builtin`, `nice`, `time`, `setsid` (prefix, no arg
  consumed), `stdbuf` (consumes one option arg like `timeout`). `env`: skip
  its option flags (`-i`, `--ignore-environment`, `-0`) and `-u NAME` /
  `--unset=NAME` before the assignment loop. Failing pins (all verified
  ALLOW, all real shell forms):
  - `sudo git commit --no-verify -m x` / `command …` / `nice …` / `time …` /
    `setsid …` / `stdbuf -o0 …` → BLOCK
  - `env -i git commit --no-verify -m x` → BLOCK
  - `sudo ironlint trust` / `command ironlint trust` → BLOCK

## Task L6 — classifier write families (`crates/ironlint-bash-gate/src/lib.rs`)

Gate: `GATE_CLASSIFIER`. **Runs after L5 merges.** Findings #9–#11, #14.

- [ ] **L6.1 dir-dest copy/move (finding #9).** In
  `cp_mv_destination_is_protected`: when the dest resolves (post-L1.1
  normalization) to a PROFILE dir (`.claude`, `.codex`) or `.git/hooks`,
  additionally block when any SOURCE operand's basename is a protected file
  basename (`settings.json`, `settings.local.json`, `hooks.json`,
  `pre-commit`). Exec-dir dests already block regardless. Failing pins (all
  verified ALLOW, all land on the rail):
  - `cp /tmp/settings.json .claude/` → BLOCK
  - `mv /tmp/settings.json .claude/` → BLOCK
  - `rsync -a /tmp/settings.json .claude/` → BLOCK
  - `install /tmp/settings.json .claude/` → BLOCK
  - `cp /tmp/pre-commit .git/hooks/` → BLOCK
  - Allow-pin: `cp foo.md .claude/agents/`, `cp .codex/hooks.json /tmp/backup`.
- [ ] **L6.2 `ln` family (finding #10).** Add `ln` to the gated families: block
  when the link-NAME operand (last non-flag token; after `-t <dir>` the dir)
  is a protected path or delete-target dir. Failing pins:
  - `ln -sf /dev/null .git/hooks/pre-commit` → BLOCK (verified: symlink
    replacement kills the floor — non-executable target = git skips it)
  - `ln -sf /dev/null .claude/settings.json` → BLOCK
  - Allow-pin: `ln -s README.md docs/readme-link`.
- [ ] **L6.3 multi-operand in-place editors (finding #11).**
  `inplace_editor_targets_protected`: check EVERY non-flag operand after the
  script argument, not just the last. Failing pins:
  - `sed -i s/x/y/ .ironlint.yml dummy.txt` → BLOCK (verified: edits both)
  - `perl -i -pe s/x/y/ .ironlint.yml dummy.txt` → BLOCK
  - Allow-pin: `sed -n s/x/y/p .ironlint.yml` (no `-i`) stays allowed.
- [ ] **L6.4 case-insensitive matching (finding #14).** Lowercase both the
  (normalized) token and the protected constants in the file/dir/floor/policy
  matchers. Verified on this machine (case-insensitive APFS): `echo x >
  .GIT/hooks/pre-commit` writes the real hook. Failing pins:
  - `echo x > .GIT/hooks/pre-commit` → BLOCK
  - `echo x > .Claude/settings.json` → BLOCK
  - Document the deliberate over-block on case-sensitive Linux filesystems
  (a genuinely distinct `.GIT` file) in the module docs.

## Task L7 — docs reconciliation

Gate: none (docs only). Findings #19, #20 + G-drift. Runs last.

- [ ] **L7.1 AGENTS.md:** W4-PARTIAL paragraph → pi is captured (3 fixtures,
  pi 0.84.3, 2026-08-27); only claude-code/opencode remain README-only. Exit
  -code invariant: exit-1-blocks is unconditional again post-D2 (or amended
  per the D2 decision). `--no-hook` flag name (spec W1-R1 says `--no-hooks`).
- [ ] **L7.2 Known-gaps paragraph (AGENTS.md + `lib.rs` module docs +
  `docs/security/trust.md`):** enumerate honestly — variable-substitution
  indirection; **interpreter writes** (`python3 -c 'open(…).write(…)'`,
  `perl -e`, `ruby`, `node -e`) are an explicitly accepted gap at the
  lazy-agent tier; home-scoped settings via file tools. Remove trust.md's
  overclaim ("A Bash write, move, delete, or chmod against any of those is
  denied" → enumerate the gated op families).
- [ ] **L7.3** Fix the false comment in `is_floor_hook_path`
  (`.git/worktrees/*/hooks/` is never read by git — common-dir only).
- [ ] **L7.4** Pin the stop-letters in `short_bundle_has_verify` tests:
  `git commit -mn x` (= `-m n`, allow), `-Fn`, `-on`, `-Mn` allow-pins so a
  dropped stop-letter can't silently survive (over-block direction only, but
  pin it).
- [ ] **L7.5 spec sync:** spec `2026-08-17` W1-R3/W1-R4 edits per D1/D2
  (done in L2.4 — verify), W4 status note updated for pi.

## Task L8 — ship

- [ ] `cargo fmt` + `GATE_FULL` green.
- [ ] Adversarial-review pass over the cumulative diff (separate reviewer
  agent, `.agents/skills/adversarial-review`).
- [ ] Re-run the Appendix probe battery against the final release binary —
  100% of block-payloads exit 2, 100% of allow-payloads exit 0.
- [ ] `./target/release/ironlint trust` (config may be touched by docs task
  if `.ironlint.yml` changes — check) — then `ironlint init` to reinstall
  the floor hook with the fixed template (the live `.git/hooks/pre-commit`
  is an artifact, not in the diff).
- [ ] Commit as a logical series: L1 / L2 / L3+L4 / L5 / L6 / L7.
- [ ] `git mv` this plan to `plans/archive/` on completion; update
  `plans/README.md`.
- [ ] Cleanup: remove `target/ironlint-bin` is fine to keep (script
  regenerates); delete any scratch. Prune the two stale
  `/private/tmp/il-hook-test/…` entries in `~/.config/ironlint/trust.json`
  (left by the review's scratch repos — read-only review artifact).

## Appendix — verified probe battery (regression corpus)

Run: `printf '%s' '<cmd>' | ./target/release/ironlint gate-bash`; exit 2 =
block, 0 = allow. Every block-payload below was observed at exit 0 (allowed)
against `3b6c3d3` and proven to do the bad thing in a live shell/git where
noted. **Each becomes an `assert_blocks`/`assert_allows` pin in L1/L5/L6 — the
battery must not live only here.**

BLOCK (currently allowed — the bugs):
`git commit --no-veri -m x` (git accepts abbreviation, hook skipped — proven)
· `git --work-tree /tmp commit -n -m x` (proven) · `git --git-dir /tmp commit
-n -m x` · `git --namespace ns commit -n -m x` · `git --config-env
core.hooksPath=MYV commit -m x` (proven) · `/usr/bin/git commit --no-verify
-m x` (proven) · `GIT_CONFIG_KEY_0=core.hooksPath /usr/bin/git commit -m x` ·
`sudo git commit --no-verify -m x` · `command git commit --no-verify -m x` ·
`nice git commit --no-verify -m x` · `time git commit --no-verify -m x` ·
`setsid git commit --no-verify -m x` · `stdbuf -o0 git commit --no-verify -m
x` · `env -i git commit --no-verify -m x` · `sudo ironlint trust` · `command
ironlint trust` · `echo x > .claude//settings.json` (proven lands) · `echo x
> .claude/./settings.json` (proven) · `echo x > .git/hooks/./pre-commit`
(proven) · `echo x > .git/hooks/../hooks/pre-commit` (proven) · `rm
.git/./hooks/pre-commit` · `cp evil.sh .pi/./extensions/` · `cp
/tmp/settings.json .claude/` · `mv /tmp/settings.json .claude/` · `rsync -a
/tmp/settings.json .claude/` · `install /tmp/settings.json .claude/` · `cp
/tmp/pre-commit .git/hooks/` · `ln -sf /dev/null .git/hooks/pre-commit`
(proven: floor dies) · `ln -sf /dev/null .claude/settings.json` · `sed -i
s/x/y/ .ironlint.yml dummy.txt` (proven: both edited) · `perl -i -pe s/x/y/
.ironlint.yml dummy.txt` · `echo x > .ironlint.ym\l` (proven lands) ·
`ir\onlint trust` (proven runs trust) · `git config core.hooksPath ''`
(proven: hooks die) · `git config core.hooksPath ' '` (proven) · `echo x >
.GIT/hooks/pre-commit` (proven on case-insensitive APFS) · `echo x >
.Claude/settings.json`

BLOCK (already correct — keep green):
`git commit --no-verify -m x` · `git commit -n -m x` · `git -c user.name=x
commit -n -m x` · `git -C /tmp commit -n -m x` · `git commit -qn -m x` · `git
config core.HooksPath /tmp/x` · `git config core.hooksPath /tmp/x` · `git -c
core.hooksPath=/t commit` · `git -cCore.hooksPath=/t commit` · `git -c
core.hooksPath=' ' commit -m x` · `git -c core.hooksPath=/dev/null commit -m
x` · `GIT_DIR=.git git commit -n -m x` · `env GIT_CONFIG_COUNT=1
GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/tmp/x git commit -m x` ·
`GIT_CONFIG_PARAMETERS=core.hooksPath=/t git commit -m x` · `bash -c "git
commit --no-verify -m x"` · `eval "git commit --no-verify -m x"` · `rm -rf
.git/hooks` · `rm -rf ~/.claude` · `cat > ~/.claude/settings.json` · `echo x
> ".ironlint.yml"` · `echo x | tee .git/hooks/pre-commit` · `cp /dev/null
.git/hooks/pre-commit` · `mv .git/hooks/pre-commit /tmp/stolen` · `truncate
-s0 .git/hooks/pre-commit` · `chmod 644 .git/hooks/pre-commit` · `echo x >
.pi/extensions/evil.sh` · `echo x > ~/.pi/agent/extensions/evil.sh` · `rm -rf
.opencode/plugins` · `ironlint trust` · `echo x > .ironlint.yml` · `sed -i
s/x/y/ dummy.txt .ironlint.yml` (last-token form — keep blocked AND note L6.3
makes position irrelevant) · `git config --unset core.hooksPath` · `git
config --add core.hooksPath /tmp/x`

ALLOW (must stay green — over-block pins):
`ls -la` · `git log -n 5` · `git merge -n` · `git cherry-pick -n abc` · `git
config --get core.hooksPath` · `git config core.hooksPath` (bare read) · `git
config --unset user.name` · `git config --add alias.st status` · `echo hi >
.claude/agents/foo.md` · `cp .codex/hooks.json /tmp/backup` · `ironlint
check` · `git -c user.name=x commit -m x` · `git -cuser.name=chris commit -m
x` · `GIT_CONFIG_GLOBAL=/dev/null git status` · `echo
GIT_CONFIG_KEY_0=core.hooksPath` (string arg, not env) · `sed -n s/x/y/p
.ironlint.yml` · `ln -s README.md docs/readme-link` · `echo "100\%"` ·
`git commit -mn x` (= `-m n`; add in L7.4)

Known non-bypasses (verified git rejects): `git commit --no-verify=true`
("takes no value").

## Out of scope

Variable-substitution indirection (`iron$(echo lint)`); interpreter writes
(`python3 -c …` — documented gap after L7.2); home-scoped settings via file
tools; in-process adapter-removal detection; worktree-vs-index content
posture. `ironlint verify`; harness live-capture for claude-code/opencode
(backlog already).
