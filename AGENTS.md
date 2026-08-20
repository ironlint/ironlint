# AGENTS.md

Guidance for AI coding agents working in this repo.

## What this is

Rust rewrite of [dynamik-dev/bully](https://github.com/dynamik-dev/bully): local CI for AI coding agents. A **check** is `files` (globs) + `run` (or `steps`) + `on` (lifecycle). ironlint matches touched files to checks, runs each command via `sh -c` with the check ABI on env + proposed content on stdin, and reads only the exit code. No per-rule engines, no severity tiers, no LLM.

Authoring guide (the canonical one — same text `init` installs as the `ironlint-config` skill): `ironlint schema`.

**Status:** 0.4 checks pipeline, bash gate (`gate-bash`) with the harness
self-defense surface, git pre-commit floor hook (`init` installs it; runs
`check --diff` at every commit — exit 2/4 block, 3 fail-open honoring
`IRONLINT_FAIL_CLOSED_ON_INTERNAL`, 1 blocks), pinned per-harness contract
fixtures (`scripts/ci-adapters.sh`). **W4 is PARTIAL**: only codex has real
live-captured payloads (apply_patch-add/update, migrated from
`tests/fixtures/codex/`); claude-code/pi/opencode dirs are README-only until
their capture procedures run (see each `fixtures/README.md`) — suites fall
back to synthetics with loud warnings, and a README-only dir is *declared*
capture-pending (loud W4-PARTIAL note, CI stays green); a fixtures dir whose
README declaration is gone hard-fails the fixture meta-tests in CI. Capture is
**back-burnered**, not dropped. **Not built:** `ironlint verify`.
**Removed:** the drift sweep / `gate-drift` (an earlier gate-after-write
attempt, deliberately abandoned — treat any docs referencing `gate-drift`,
drift-stamp.json, or violations.json as stale). Next design:
`specs/2026-08-17-git-floor-hook-and-self-defense-design.md`.

## Commands

```bash
cargo build --release                       # ./target/release/ironlint
./target/release/ironlint check              # bare = repo-wide sweep
cargo test                                  # all workspace tests
cargo test -p ironlint-core                 # core only / -p ironlint-cli for CLI
cargo test --test cli_e2e_gates             # one integration test file
cargo clippy --all-targets -- -D warnings
cargo fmt
bash scripts/ci-coverage.sh                 # per-file ≥80% region-coverage gate (matches CI)
```

## Invariants — do not break

- **Exit codes** (`commands/check.rs`): `0` pass, `1` config/load error, `2` block (check exited 1–125), `3` internal (127/timeout/signal — never a silent pass; adapters fail-open by default, opt-in `IRONLINT_FAIL_CLOSED_ON_INTERNAL=1`), `4` untrusted config (fail-closed at adapters; run `ironlint trust`). Consumed by CI and adapters.
- **Trust** lives at the CLI `check` layer only (`~/.config/ironlint/trust.json`, keyed by canonical config path, hash covers config bytes + `.ironlint/scripts/`). `IronLintEngine::load` stays pure; read-only commands (`validate`, `explain`, `show-resolved-config`, `doctor`) never enforce trust.
- **Scope matching** (`config/scope.rs`): bare glob without `/` matches at any depth (`*.py` → `**/*.py`). Deliberate bully parity — don't "fix" it.
- **Verdict JSON** is a public surface: `Verdict`, `Block`, `GateError`, `Status` locked at `SCHEMA_VERSION = 6`; bump to change. Telemetry records are versioned separately (currently v5).
- **Binary name** is `ironlint`. `Cargo.lock` is committed; CI builds `--locked`.
- **Check ABI** (locked): `$IRONLINT_FILE`, `$IRONLINT_FILES`, `$IRONLINT_ROOT`, `$IRONLINT_EVENT` (write|pre-commit), `$IRONLINT_TMPFILE`, `$IRONLINT_BIN`, `$IRONLINT_PROPOSED_MANIFEST`, proposed content on stdin. Paths travel as env values, never spliced into `run`.
- **Suppression**: an `ironlint-disable: <check-id>` line directive silences that check for the whole file; directive ends at whitespace/`*`/`/`.
- **Bash gate** (`ironlint gate-bash`, crate `ironlint-bash-gate`): NOT a check, NOT trust-gated, runs config-less. Exit `0` allow / `2` block; anything else is fail-closed at adapters. Blocks `ironlint trust`, Bash writes to the policy surface (`.ironlint.yml`, `.ironlint/scripts/`), the adapter installation surface (`.claude/settings*.json`, `.codex/hooks.json` — exact FILES gate all write families; `.pi/extensions`, `~/.pi/agent/extensions`, `.opencode/plugins` — EXEC dirs gate all write families, since writing there installs a rail-bypass executable; the broad `.claude`/`.codex` profile dirs gate only the DELETION family, so Bash-authoring a skill file under `.claude/` stays legal — parity-tested against `adapter::adapter_install_surface`), the git floor hook (`.git/hooks/pre-commit` + the `.git/hooks` parent dir, deletion family), and the git-bypass forms (`--no-verify` blocks for every hook-running subcommand; `-n`/short-bundles block for `commit` only — `merge`/`pull` `-n` is `--no-stat`, `cherry-pick`/`revert` `-n` is `--no-commit`; `-c core.hooksPath=…` and `config` hooksPath mutations block, case-insensitively; `GIT_CONFIG_KEY_i`/`GIT_CONFIG_PARAMETERS` env injection of `core.hooksPath` blocks, case-insensitively; `config --get`/bare-key READS stay allowed), plus `rm`/`chmod`/`chown`/`rmdir`/`unlink`/`truncate` against any protected path. Known gaps: variable-substitution indirection (documented, out of scope); home-scoped settings are reachable via file tools (project-scoped settings are repo paths — cover them with a normal check).

## Process rules

- Bug fixes start with a failing test; that test becomes regression coverage.
- After completing a coding task, request code review from a separate agent.
- Rust files under `crates/*/src/` must meet ≥80% **region** coverage; CI enforces per-file via `scripts/ci-coverage.sh`.
- Cognitive complexity ≤15 per function (clippy `cognitive_complexity`, warned at crate roots). Refactor over annotate.
- Mutation testing (`cargo mutants`) is local/ad-hoc investigation, not a CI gate.
- Delete build artifacts your task produced (`target/release` binaries, scratch diffs, mutants output); the iterating `target/` stays.
- **Adapter contract fixtures** (`adapters/<harness>/fixtures/`, W4): on any harness release that touches hooks/plugin payloads, run the `adapter-drift-audit` skill for that harness; if payloads changed, recapture fixtures (see `adapters/<harness>/fixtures/README.md`) and bump `harness_version`. A fixture without a parseable provenance header fails the contract tests; a missing fixture falls back to embedded synthetics with a loud capture-pending warning; a fixtures dir that loses its README declaration hard-fails the meta-tests in CI. `scripts/ci-adapters.sh` runs all four contract suites.

## Architecture

Cargo workspace, three crates. Binary: `ironlint`.

- **`ironlint-core`** — library: `config` (parse/checks/`extends`/scope), `diff` (unified-diff parser), `engine::gate` (spawn `sh -c`, stdin, timeout, classify exit into `GateOutcome`), `runner` (load → dispatch per lifecycle → `Verdict` → telemetry), `trust`, `verdict`, `disable`, `telemetry` (`.ironlint/log.jsonl`), `watch` (log TUI), `adapter` (adapter install/uninstall + `.ironlint-adapter.json` sidecar).
  - `config::extends::resolve`: cycle-detected DFS; **local checks win on collision**.
  - `config::parser` rejects pre-0.3 configs (`schema_version:`/`rules:`/`trust:` keys) with a curated error — no migration path.
- **`ironlint-cli`** — thin binary; `cli.rs` clap subcommands (`check`, `validate`, `init`, `explain`, `show-resolved-config`, `doctor`, `trust`, `gate-bash`, `update`, `schema`, `watch`); `commands/*` are one-function adapters into core. CLI tests use `assert_cmd`.
- **`ironlint-bash-gate`** — leaf crate, zero deps: the pure-Rust Bash classifier behind `ironlint gate-bash`.

**Lifecycles.** `on: [write]` (default): per matching file, proposed content on stdin. `on: [pre-commit]`: one invocation per check over the whole matching set, `$IRONLINT_FILES` populated, stdin empty. Bare `check` sweep batches pre-commit checks (one spawn per check); write checks run sequentially per file. Timeout: `IRONLINT_TIMEOUT` env > `execution.timeout_secs` (default 30, clamped ≥1). No sandboxing.

Test fixtures: `tests/fixtures/` at repo root, relative paths from crate tests.

Design history: `specs/` (authoritative: `specs/2026-06-28-ironlint-checks-pipeline-design.md`); plans: `plans/`.

<!-- graft:start -->
## Graft — repo context graph

This repo is indexed in `graft/`: small linked markdown nodes that explain each
system and carry exact file:line spans, kept in sync with the code through git.

For ANY task here — understanding how something works, finding where code lives,
or scoping a change — get context from the graph before grepping or opening
source files. Re-ask freely (it's cheap) and reuse literal identifiers you
already have (symbol, error string, file name) as the query. New to this repo?
Run `graft map` first — a token-budgeted orientation (dir clusters, hubs,
hotspots), no LLM, no key.

- Run `graft ask "<your question>" --source` → ranked nodes with the relevant
  code spans inlined (each hit's ≤8-line crux by default; `--full` for whole
  definitions when the crux isn't enough). Match the tool to the task shape:
  for understanding or editing, the top node IS the answer — cite its
  `covers:` file:line spans and edit straight from `--source`. For
  exhaustive tasks ("every occurrence / every caller of this pattern"), ranked
  results are top-N, not complete — run `graft grep "<literal>"` instead
  (exhaustive over indexed files, grouped by enclosing symbol), falling back
  to raw `grep -rn` only for unindexed files.
- `graft skeleton <file>` → every definition's signature + span, ~10× cheaper
  than reading the file; use it to skim an API surface.
- `graft callers <symbol>` gives precomputed, exact edges — who calls this.
  Add `--direction out` for what it calls, or `--depth N` to walk
  transitively for the full blast radius. For structural questions, skip
  ranking and use this directly.
- Or browse: `graft/INDEX.md` lists every node; follow the links.
- Monorepos and folders of multiple repos rank fairly across sub-projects —
  hits carry `[scope/]` labels naming which one they're from. Narrow with
  `graft ask "<task>" --in <scope>/` once you know where you're working.

If a returned span is truncated ("+N more lines"), open the file at that exact
range before finalizing. Only open source files when a node genuinely lacks a
needed detail, and then at the exact file:line the node points to — never
re-read whole files.

After big code changes, refresh the graph with `graft build` (deterministic,
no API key, $0).
<!-- graft:end -->
