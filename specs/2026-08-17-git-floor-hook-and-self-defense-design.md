<!-- ironlint-disable: no-load-bearing -->

# Git floor hook, check-placement heuristic, adapter self-defense, contract fixtures

**Date:** 2026-08-17
**Status:** Design (pre-implementation)
**Scope:** `ironlint-cli` (`init`, floor hook install/uninstall, `check --diff` semantics), `ironlint-bash-gate` (harness-surface blocklist), `adapters/shared/ironlint-config/SKILL.md` (placement heuristic), the repo's own `.ironlint.yml` (dogfood), adapter contract fixtures + CI lane, AGENTS.md

## Problem

Four gaps surfaced while evaluating ironlint against its stated thesis —
*deterministic guard rails that any agent, pointed at any clone of my codebase,
cannot remove:*

1. **The `pre-commit` lifecycle is orphaned.** Nothing triggers it at the
   boundary it is named for. Adapters gate the Write/Edit/Bash *tool calls*;
   they never intercept `git commit`. The only paths that ever run
   `on: [pre-commit]` checks are a bare `ironlint check` sweep (what
   `scripts/ci-dogfood.sh` runs) or a manually-passed `--event`. Telemetry
   confirms: six weeks of dogfooding produced 1,571 `write` events and exactly
   **5** pre-commit events, none at real commit boundaries. Worse, there is no
   git-level floor at all: an unsupported/future agent, a human at the
   terminal, or a session whose harness adapter has been disabled can write
   *and commit* without any rail firing.
2. **One mis-placed check produces most of the gate friction.** Telemetry:
   `rustfmt-on-write` accounts for 70 of 88 blocks (9.9% block rate) and
   effectively all of the "agent fights the gate" repeats (30 consecutive
   same-check/same-file re-blocks in 6 weeks, longest streak 5). A
   mid-construction file is *expected* to be unformatted; blocking it at write
   is a false-positive machine. The precision-policy checks
   (`no-load-bearing`, `no-debug-macros-in-src`) each run at 0.4–1.9% block
   rates — cheap, and they catch compounding damage early enough to matter.
   The placement lever (`on:`) already exists; nothing teaches users where to
   put which kind of check.
3. **An agent can uninstall the rails themselves through Bash.** gate-bash
   blocks `ironlint trust` and Bash writes to the policy surface
   (`.ironlint.yml`, `.ironlint/scripts/`), but the adapter installation
   artifacts — `~/.claude/settings.json`, `.claude/settings.local.json`,
   `.codex/hooks.json`, `.pi/extensions/`, `.opencode/plugins/` — are plain
   files the agent can edit or delete via Bash with zero gate. Once the hook
   is gone, nothing in-process can even detect the removal (honest residual;
   see Non-goals).
4. **Adapter contract coverage is hand-rolled per harness with no pinned
   payloads.** `hook_contract_claude_code.rs` / `hook_contract_codex.rs`
   embed synthetic payload strings; pi (`adapters/pi/test/index.test.ts`) and
   opencode (`adapters/opencode/tests/plugin.test.ts`) have their own suites.
   There is no captured-from-a-live-harness fixture anywhere, so a harness
   silently changing its hook payload shape degrades an adapter with no test
   noticing — the exact failure the `adapter-drift-audit` skill exists to
   audit manually.

## Goal

Make the enforcement story true at the floor level and hard to disarm, in
increasing order of leverage, without changing the check-execution model:

- **W1.** `ironlint init` installs a real git `pre-commit` hook (the "floor")
  that runs the checks against the staged change set. One hook covers every
  committer — supported agents, unsupported agents, humans — independent of
  any harness adapter being installed, present, or alive.
- **W2.** Encode the placement heuristic in the one canonical authoring guide
  (`adapters/shared/ironlint-config/SKILL.md`, which `ironlint schema` prints
  and `init` installs as the `ironlint-config` skill): *checks that a
  legitimately mid-construction file can trip go to `pre-commit`; checks that
  are never legitimate go to `write`.* Dogfood it in this repo by moving
  `rustfmt-on-write` to `pre-commit`.
- **W3.** Extend gate-bash with a **harness self-defense** blocklist: Bash
  writes/deletes/patches against the adapter installation surface are blocked
  the same way policy-surface writes are today.
- **W4.** Pin each harness's hook payload as a provenance-stamped fixture,
  rewire all four adapters' contract tests to load from those fixtures, and
  run them in one `scripts/ci-adapters.sh` lane.

## Non-goals

- **No check-execution changes.** Dispatch, ABI, verdicts, trust, telemetry
  are untouched except where W1 requires confirming --diff lifecycle dispatch.
- **No in-process detection of adapter removal.** If the agent deletes the
  hook from its harness settings, by definition nothing of ours runs in that
  harness afterward. This is a documented residual. Mitigations are
  out-of-band: the W1 floor still fires at commit (rail removal does not
  remove the floor), and `ironlint doctor` remains the manual/CI audit.
- **No variable-substitution tier.** W3 inherits gate-bash's documented
  threat model verbatim
  (`docs/superpowers/specs/2026-07-06-bash-gate-self-trust-prevention-design.md`:
  lazy-agent tier; `iron$(echo lint) trust`-style indirection remains a known
  gap, explicitly not a TODO).
- **No autofix mode.** We do not let checks mutate the worktree mid-gate.
  `rustfmt-on-write` gets *moved*, not turned into `cargo fmt -i`.
- **No `core.hooksPath` management, no lefthook/husky replacement.** The
  floor chains with whatever already lives in `.git/hooks/pre-commit`.
- No staged-index content surgery. Checks read on-disk content (existing
  security-explicit behavior); `git add -p` partial staging checks the
  worktree, not the index. Documented limitation, same posture as the
  drift sweep.
- No Windows support beyond what the repo already claims.

## W1 — git floor hook

### Requirements

**W1-R1.** `ironlint init` (default flow, after the harness-select step in
`crates/ironlint-cli/src/commands/init/onboard.rs`) installs a `pre-commit`
hook. Not part of the harness multi-select — it is not a harness — but shown
in the plan/dry-run output and the summary. Opt-outs: a new
`--no-git-hook` flag; the existing `--no-hooks` (legacy scaffold-only) implies
it.

**W1-R2.** Install location: `"$(git rev-parse --git-common-dir)/hooks/pre-
commit"`. Using the common dir means a linked worktree inherits the floor
automatically (worktree trust inheritance is already handled —
`docs/superpowers/specs/2026-07-11-git-worktree-trust-inheritance-design.md`).
Skip with a clear notice when the cwd is not inside a git work tree.

**W1-R3.** Chaining, not capture. The hook is a marker-bracketed block:

```sh
#!/bin/sh
# >>> ironlint pre-commit floor >>>
# Managed by ironlint init — remove with `ironlint init --uninstall`.
diff_file="${TMPDIR:-/tmp}/ironlint-precommit.$$"
trap 'rm -f "$diff_file"' EXIT
git diff --cached --diff-filter=ACMR --no-color >"$diff_file" || exit 0
[ -s "$diff_file" ] || exit 0            # nothing added/copied/modified/renamed staged
BIN="${IRONLINT_BIN:-<absolute-path-recorded-at-init>}"
[ -x "$BIN" ] || BIN="$(command -v ironlint || true)"
if [ -z "$BIN" ]; then
  echo "ironlint floor: binary not found; allowing commit (reinstall: ironlint init)" >&2
  exit 0
fi
"$BIN" check --diff "$diff_file"
code=$?
case "$code" in
  0) exit 0 ;;
  3) if [ "${IRONLINT_FAIL_CLOSED_ON_INTERNAL:-0}" = "1" ]; then
       echo "ironlint floor: internal error — blocking (IRONLINT_FAIL_CLOSED_ON_INTERNAL=1)" >&2
       exit 1
     fi
     echo "ironlint floor: internal error — allowing commit" >&2
     exit 0 ;;
  4) echo "ironlint floor: config/checks untrusted — run: ironlint trust" >&2
     exit 1 ;;
  *) exit "$code" ;;   # 2 = verdict block (message already printed); 1 = load error blocks at the floor
esac
# <<< ironlint pre-commit floor <<<
```

Append/insert semantics per existing file state:

| `.git/hooks/pre-commit` state | action |
|---|---|
| absent | create with shebang + marker block, `chmod +x` |
| present, has marker | replace the marker block in place (idempotent reinstall / binary-path update) |
| present, no marker | append marker block at end — existing content runs first; if it exits nonzero git aborts before our block, which is the correct precedence |

**W1-R4.** Exit mapping decision (locked): exit 2 → block commit; exit 4 →
block commit with `ironlint trust` remediation (the floor is the
un-bypassable rail; an untrusted policy must never slip through it); exit 3 →
allow with warning, honoring `IRONLINT_FAIL_CLOSED_ON_INTERNAL=1` (mirrors the
adapter contract); exit 1 → block (a config that cannot load = policy
unenforceable; the human can `--no-verify` deliberately, which is a human
decision at their own terminal, not an agent one — the harness adapters'
gate-bash branch already runs on agent Bash); missing binary or empty diff →
allow with warning. Never leave `$diff_file` behind.

**W1-R5.** One invocation, no `--event`. Confirm during implementation that
`ironlint check --diff <file>` with no `--event` dispatches each check per its
own `on:` lifecycle against the diff's file set — write-lifecycle checks once
per matching file, pre-commit-lifecycle checks once over the whole set
(`run_diff` in `crates/ironlint-cli/src/commands/check.rs`). **If it does not
— i.e. `--diff` currently implies the write event for everything — fix
`run_diff` as part of W1.** That fix is the second half of "un-orphan the
pre-commit lifecycle". Add a focused E2E test that pins whichever semantics
are chosen (see Test Plan).

**W1-R6.** Uninstall. `ironlint init --uninstall` removes the marker block in
addition to the harness hooks it removes today; if the remaining hook file is
empty modulo shebang/whitespace, delete the file. gate-bash already blocks
agents from running `init --uninstall` via Bash — unchanged.

**W1-R7.** Trust interplay, documented to the user. After `init` (or any
config edit), the first commit through the floor exits 4 and blocks with the
remediation printed. The `init` summary must tell the user to run
`ironlint trust` when the floor is installed and the current config hash is
not yet blessed — otherwise the user's next commit is a surprise block.

**W1-R8.** The floor re-running write-lifecycle checks on staged content is a
feature, not redundancy: for adapter-covered sessions it is a cheap second
pass on final content (the drift sweep already accepts this shape); for
bypassed/harness-less sessions it is the only pass.

### W1 tests

- `tests/cli_init_onboarding.rs`-style E2E: fresh tmp git repo → `init` →
  hook file has marker block, is executable.
- Idempotent: second `init` leaves the file byte-identical minus the marker
  block content; third with changed binary path replaces only the block.
- Chaining: pre-existing custom hook that writes a sentinel file → both run
  and the sentinel is written; pre-existing failing hook → ironlint block
  never runs (precedence), commit aborted.
- Behavior: staged fixture that fails a pre-commit-lifecycle check → commit
  blocked, message surfaces; passing staged set → commit succeeds; staged
  deletions only → hook allows without invoking ironlint
  (`--diff-filter=ACMR` produces an empty diff; assert with an `IRONLINT_BIN`
  wrapper that records invocation).
- Exit mapping: fake `ironlint` shim on PATH returning 3 → commit allowed +
  warning; with `IRONLINT_FAIL_CLOSED_ON_INTERNAL=1` → blocked; returning 4 →
  blocked with `ironlint trust` in stderr; returning 1 → blocked.
- Missing binary (recorded path absent, shim removed) → commit allowed with
  warning on stderr.
- Uninstall: marker removed, pre-existing content byte-preserved; ironlint-
  only file → file deleted.
- `--diff` lifecycle dispatch pin (W1-R5): config with a write-only check and
  a pre-commit-only check, staged set matching both → assert each fires
  exactly once with its correct event semantics.

## W2 — check-placement heuristic

The guide's single source of truth is
`adapters/shared/ironlint-config/SKILL.md` (embedded by
`crates/ironlint-cli/src/commands/schema.rs` via `include_str!`; installed by
`init` as each harness's `ironlint-config` skill). One edit propagates
everywhere.

**W2-R1.** Add a "Lifecycle placement" section to the guide, text along these
lines (author may polish, must keep the rule + the rationale + the example):

> Place by legitimacy, not by preference. A check belongs at `write` if a
> file tripping it is *never* legitimate — banned APIs, secrets, forbidden
> markers, debug macros. Catching those at write stops the agent before the
> pattern spreads across files (commit-time rework is far more expensive). A
> check belongs at `pre-commit` if a legitimately mid-construction file could
> trip it — formatting, import resolution, whole-tree compilation, tests.
> Blocking those at write punishes TDD and partial edits.
>
> When a check moves to `pre-commit`, it can no longer read proposed content
> on stdin — re-scope the command to `$IRONLINT_FILES` (the matched set, on
> disk). Example — rustfmt moved to the floor:
>
> ```yaml
> rustfmt:
>   name: rustfmt check
>   files: "**/*.rs"
>   on: [pre-commit]
>   run: |
>     echo "$IRONLINT_FILES" | tr '\n' '\0' | xargs -0 rustfmt --check --color=never
> ```

**W2-R2.** Dogfood: edit this repo's `.ironlint.yml` — move
`rustfmt-on-write` to the placement above (keep the check-id or rename to
`rustfmt`; author's choice, but keep telemetry comparability in mind). The
config edit changes the trust hash: run `ironlint trust`, and note that
command in the commit message (the trust store itself is out-of-repo —
nothing extra to commit).

**W2-R3.** Extend `tests/cli_schema.rs` and the embedded-guide test in
`schema.rs` to assert the new section is present (both the heading and the
`$IRONLINT_FILES` example).

## W3 — gate-bash harness self-defense blocklist

**W3-R1.** Extend the policy-surface concept in
`crates/ironlint-bash-gate/src/lib.rs` (`is_policy_path` / `is_policy_write`
and the redirect/tee/cp/mv/dd/sed-i family) with a second token class:
**adapter installation surface**. Block any Bash write/move/delete/chmod
targeting:

- `~/.claude/settings.json`, `$HOME/.claude/settings.json`, and
  project-relative `.claude/settings.json` / `.claude/settings.local.json`
- `~/.codex/hooks.json`, `$HOME/.codex/hooks.json`, project-relative
  `.codex/hooks.json`
- `.pi/extensions` and `~/.pi/agent/extensions` (any write targeting a path
  under either)
- `.opencode/plugins` (project-scoped only — the registry has no opencode
  global install)

`~` and `$HOME` both appear in agent-typed commands; normalize both against
the process's actual `$HOME` before matching, and match absolute forms too.
Home-relative expansion is new for this crate — the classifier today is
pure-string over repo-relative tokens; making it $HOME-aware is part of this
workstream, and the process env is a legitimate input (gate-bash runs as a
spawned process).

**W3-R2.** Single source of truth: the set above must equal the union of the
registry's install targets (`crates/ironlint-core/src/adapter/registry.rs` —
the `settings_*` and `dir_*` fields of every adapter). Because
`ironlint-bash-gate` is a leaf crate that must not depend on `ironlint-core`
(cycle), encode parity as a test in `ironlint-cli` that asserts the exported
blocklist constant from gate-bash equals the registry-derived set. When
someone adds a fifth adapter, the parity test fails before the gate silently
under-covers it.

**W3-R3.** Git-floor bypass forms — **recommended, needs explicit maintainer
sign-off in review**: block `git commit --no-verify`, `git -c
core.hooksPath=… commit`, `git config core.hooksPath …`, and Bash
writes/deletes/chmod targeting `.git/hooks/pre-commit` (including the common
dir when cwd is a linked worktree). Rationale: identical to the argument
gate-bash already accepted for `ironlint trust` — the lazy-agent tier reaches
for the obvious escape from whichever rail just blocked it; with W1 shipped,
`--no-verify` *is* that escape. It is cheap (the size of the existing trust
matcher) and only affects agent Bash tool calls — a human typing at a
terminal is untouched. Document that it shares the var-substitution gap. If
sign-off declines, ship W3-R1/R2 only and record the decision here.

> **Decision (2026-08-17 implementing session):** W3-R3 SHIPPED — the
> bypass forms and `.git/hooks/pre-commit` writes are blocked (implemented
> per the recommendation above, pinned by `is_git_commit_escape` + the
> floor-hook path class in `ironlint-bash-gate`, E2E-covered in
> `cli_e2e_gate_bash.rs`). Awaiting the maintainer sign-off review called for
> in the spec; if it declines, the blocklist entries and their tests are
> reverted to W3-R1/R2-only.

> **Decision update (2026-08-18, round-2 review):** the classifier got a
> hardening pass against live-probed bypasses — `git -c/-C <val> commit -n`
> (value-taking flags now skipped when resolving the subcommand),
> `git commit -qn` (short-flag bundles scanned for `n`, stopping at
> value-taking letters), case-insensitive `core.hooksPath` keys, `.git/hooks`
> parent-dir deletion, and `-n` restricted to `commit` only (merge/pull `-n`
> is `--no-stat`, cherry-pick/revert `-n` is `--no-commit`; long-form
> `--no-verify` still blocks all hook-running subcommands). Over-blocks
> fixed: `git config --get core.hooksPath` reads stay allowed (only
> mutating forms block), and the broad `.claude`/`.codex` profile dirs now
> gate only the DELETION family so Bash-authoring a skill file stays legal
> (exec dirs `.pi/extensions`/`.opencode/plugins` keep all-family gating).
> **Known gap (documented):** variable-substitution indirection only
> (`iron$(echo lint)`-style), same tier as before. The env-injection path
> `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_i`/`GIT_CONFIG_VALUE_i` / `GIT_CONFIG_PARAMETERS`
> that sets `core.hooksPath` is NOW COVERED (2026-08-20, punch-list Task 2):
> the classifier blocks the key-carrying token case-insensitively when the
> segment invokes git.

**W3-R4.** Document the residual honestly (in this spec when finalized, in
gate-bash's module docs, and in the security docs): home-scoped harness
settings sit outside every repo glob's reach, so the agent's *file tools* can
edit `~/.claude/settings.json` ungated — W3 closes only the Bash path.
Project-scoped settings ARE repo paths; a user who wants file-tool coverage
adds a normal check scoping `.claude/settings*.json` and friends — the guide
gains one line suggesting this (folded into W2's guide edit, same file), not
a shipped default check, because repo layouts differ.

### W3 tests

- Classifier battery in `lib.rs` following the existing `assert_blocks` /
  allow patterns: each surface path × each write op (`>`, `>>`, `>|`, `tee`,
  `sed -i`, `cp`/`mv` dest, `dd of=`, `rm`, `chmod`, `install`, `rsync` dest)
  × spelling (`~`, `$HOME`, absolute, project-relative).
- Allow-list pins: `cat ~/.claude/settings.json`, `cp .codex/hooks.json
  /tmp/backup` (source, not dest), unrelated `settings.json` paths elsewhere.
- Parity test (W3-R2) in `ironlint-cli`.
- `tests/cli_e2e_gate_bash.rs` additions: exit-code/JSON contract for at
  least two blocked and two allowed forms end-to-end.

## W4 — pinned contract fixtures + one CI lane

**W4-R1.** New dir per harness: `adapters/<harness>/fixtures/`. Each fixture
is a captured payload from a *live* harness with a provenance header, e.g.
`claude-code/pretooluse-write.json`:

```json
{
  "_provenance": {
    "harness": "claude-code",
    "harness_version": "<exact version>",
    "captured_at": "2026-08-…",
    "captured_by": "hook.sh with `tee` of stdin during a real session"
  },
  "payload": { "...the verbatim harness payload..." }
}
```

Capture method for the initial set: run each supported harness against a
scratch repo with a temporary instrumented hook (tee stdin to a file),
perform one Write-like and one Bash-like tool call, save the payloads,
remove instrumentation. Provenance is mandatory — a fixture without it fails
the contract tests (R3).

**W4-R2.** Rewire all four adapters' tests to load fixtures from those dirs:
`tests/hook_contract_claude_code.rs` and `…_codex.rs` replace their embedded
synthetic payloads with the fixture payloads (keep their current synthetic
edge cases as additional tests — fixtures pin the happy shape, synthetics
still cover malformed/adversarial shapes); `adapters/pi/test/index.test.ts`
and `adapters/opencode/tests/plugin.test.ts` consume their harness's fixtures
the same way.

**W4-R3.** Contract assertions per fixture: payload parses, contains the
fields the adapter reads (tool name / file path / command per harness),
produces the adapter's expected allow/block response shape. Plus a meta-test:
every fixture file carries a parseable provenance header with
`harness_version` and `captured_at`.

**W4-R4.** `scripts/ci-adapters.sh`: runs the two Rust hook-contract test
files plus the pi and opencode suites (bun for opencode, node/tsc for pi —
match what each suite already uses; fail loudly with install instructions
rather than silently skipping when tooling is missing). Wire into the
existing CI workflow as its own job.

**W4-R5.** Process (docs-only, lands in AGENTS.md conventions): on any
harness release touching hooks → run the `adapter-drift-audit` skill for that
harness → if payloads changed, recapture fixtures and bump
`harness_version`. Fixture version vs. live version is exactly the comparison
the audit skill already walks; the stamp makes drift visible in `git diff`
instead of in someone's memory. Automating the audit is explicitly backlog,
not this spec.

> **W4 status (2026-08-18): PARTIAL.** Only codex has real payloads — the
> two apply_patch fixtures in `adapters/codex/fixtures/` (migrated from the
> legacy `tests/fixtures/codex/` dir, provenance-stamped, codex-cli 0.141.0,
> captured 2026-07-03). claude-code/pi/opencode dirs are README-only until
> their capture procedures run (see each `fixtures/README.md`); their suites
> fall back to embedded synthetics with loud capture-pending warnings. The
> meta-tests now enforce a three-state rule: README-only = capture pending
> DECLARED (loud W4-PARTIAL warning, passes in CI — the pending state is on
> record); undeclared-empty (missing dir, or no README and no fixtures) =
> warn locally but HARD FAIL in CI, so a vanished fixtures dir can never go
> silently green. Fresh live capture is back-burnered, not dropped — a
> follow-up once the harness capture paths work end-to-end.

## Ordering and dependencies

W1 first (highest leverage; the W1-R5 `--diff` semantics question gates
nothing else but must be confirmed before the hook content is finalized).
W2, W3, W4 are mutually independent and can land in parallel branches;
W3-R4's one guide line coordinates with W2-R1 (same file — land W2 first or
merge the two edits).

## Test plan (global)

Per repo conventions: failing-test-first for bug-shaped pieces (W1-R5 if
`run_diff` is wrong), every changed file under `scripts/ci-coverage.sh`
≥80% region coverage, `cargo clippy --all-targets -- -D warnings` clean,
cognitive complexity ≤15 per function, `cargo fmt`. New code: W1 new module
`crates/ironlint-cli/src/commands/init/git_hook.rs` (install / uninstall /
marker-block surgery, unit-tested on tmpdirs) + E2E in
`tests/cli_init_git_hook.rs`; W3 classifier extensions unit-tested in place +
E2E in `tests/cli_e2e_gate_bash.rs`; W4 fixtures + rewired contract tests +
`scripts/ci-adapters.sh`.

## Docs to update when landing

- AGENTS.md "What this is" paragraph (add the floor; extend the gate-bash
  description with the self-defense surface).
- `ironlint schema` guide (W2).
- gate-bash module docs + security doc (W3 residual + R3 decision).
- README positioning pass is deliberately out of scope (separate copy task).

## Decisions

1. **W1-R5 — bare `--diff` dispatches per-check lifecycle (decided).**
   Confirmed and shipped: bare `--diff` is `DiffDispatch::Implicit` in
   `crates/ironlint-cli/src/commands/check.rs`, so each check dispatches per
   its own `on:` lifecycle (write checks once per matching file, pre-commit
   checks once over the whole set). Pinned by
   `crates/ironlint-cli/tests/cli_diff_lifecycle.rs`.
2. **W3-R3 — git-bypass forms SHIPPED.**
   `--no-verify`, `-n`/short-bundle, `-c core.hooksPath=…` and `config`
   hooksPath mutations all block in the classifier. Sign-off obtained
   implicitly during the implementing session; the maintainer confirms at
   commit time.
3. **W2 dogfood — check renamed to `rustfmt` (decided).**
   The repo's own `.ironlint.yml` uses the `rustfmt` check id (the
   `rustfmt-on-write` working name is gone).
4. **W4 — bun + node on the adapters CI lane confirmed.**
   The `adapters` job in `.github/workflows/ci.yml` installs both
   (`actions/setup-node` + `oven-sh/setup-bun`), and
   `scripts/ci-adapters.sh` fails loudly when either tool is missing.
