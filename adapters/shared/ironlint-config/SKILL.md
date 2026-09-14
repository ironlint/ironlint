---
name: ironlint-config
description: Authors, modifies, or removes checks in an ironlint .ironlint.yml. Use when the user says "add an ironlint check for X", "ban Y", "tighten <check-id>", "stop checking <check-id>", "remove <check-id>", "change the scope of a check", or asks how to write an ironlint config.
license: MIT
metadata:
  author: dynamik-dev
  version: 1.3.0
---

# IronLint policy authoring

## v1 policy format (acceptance contract)

Use `version: 1` for the current acceptance-gate contract. This is a
different format from the transition-era legacy guide below.

```yaml
version: 1

execution:
  timeout_secs: 30
  total_timeout_secs: 300

checks:
  no-debug:
    files: ["src/**/*.txt"]
    on: [change, accept]
    run: "! grep -n 'DEBUG' \"$IRONLINT_ROOT/src/example.txt\""

  tests:
    run: ./scripts/test
```

`version: 1` is required. `checks` must be a nonempty mapping; every check
must have a nonempty `run`. A check's `files` is an optional nonempty glob or
list of globs, and existing bare-glob behavior remains (`*.rs` matches at any
depth). `on` defaults to `[accept]`; an explicit list may contain `accept`, or
`change` and `accept` in either order. Duplicate or unknown events are errors,
and an explicit list must include `accept`.

Acceptance runs every configured check, regardless of `files` or changed paths.
For `change`, only checks opted into `change` run; known changed paths filter
those checks by `files`, while unknown paths run all change checks. Checks are
selected once per invocation in ID order, and the command owns any file
iteration. `files` is a feedback trigger, not a sandbox. There are no v1
`steps`, `extends`, check-ranking fields, suppression, or feedback-only checks.

The `execution` block is optional. It defaults to 30 seconds per check and 300
seconds per invocation; both values must be positive integers. Put command
sequences in reviewed scripts.

### v1 command ABI

Each `run` is executed as `sh -c` from `$IRONLINT_ROOT`, with stdin closed.
Commands inspect the evaluated tree on disk, not proposed content on stdin. For
example, the `no-debug` check above reads the actual file:

```yaml
run: "! grep -n 'DEBUG' \"$IRONLINT_ROOT/src/example.txt\""
```

The child environment starts cleared, then retains the safe `PATH`, `HOME`,
`LANG`, `TZ`, `TMPDIR`, and `LC_*` values plus exactly these IronLint values:
`IRONLINT_ROOT`, `IRONLINT_EVENT` (`change` or `accept`), and `IRONLINT_BIN`.
Inherited `IRONLINT_*` values are cleared before those three are set. Legacy
`IRONLINT_FILE`, `IRONLINT_FILES`, `IRONLINT_TMPFILE`, and
`IRONLINT_PROPOSED_MANIFEST` are not supplied under v1.

Exit 0 is a pass. Exit 1–125 is a check failure; IronLint continues to
collect other ordinary failures. Exit 126/127, signal termination, or a
timeout is an execution error and stops remaining checks. The CLI reports
configuration/input errors as exit 1, untrusted policy as 4, check failures as 2,
execution errors as 3, and a complete pass as 0.

## Transition-era legacy format (0.4)

The remainder of this guide applies only to configs without `version: 1`. It
documents the retained write/pre-commit behavior while projects migrate to v1.

A legacy ironlint policy lives in `.ironlint.yml` at the project root. A
**check** is a file scope plus a shell command (or sequence of steps):

```yaml
checks:
  no-debug:
    files: "**/*.ts"          # glob, or a list of globs
    run: "! grep -n 'DEBUG'"  # proposed content arrives on stdin; nonzero = block
```

- `files` — the glob(s) the check watches. A bare pattern with no `/` (e.g. `*.py`) also matches at any depth.
- `run` — a shell command handed to `sh -c`. **Any nonzero exit (1–125) blocks the edit**; exit 0 passes. `126`/`127`/timeout are treated as a broken check, not a block.
- `steps` — alternative to `run`: a sequence of `{name?, run}` steps, all fed the same stdin. The first nonzero step blocks.
- `on` — lifecycle events: `[write]` (default) fires per file on every agent write; `[pre-commit]` fires once over the selected matching file set (see ABI below). Use `on: [write, pre-commit]` to fire at both.
- `name` — optional human-readable label. Parsed and reserved, but **not yet surfaced** in block messages or `ironlint explain` — it's a no-op today. (A `steps` entry's `name`, by contrast, *is* reported as the block's `step` field.)

## ABI — what every check receives

- `$IRONLINT_FILE` — absolute path of the single file under check (set for `write`; not set for `pre-commit`).
- `$IRONLINT_FILES` — newline-joined list of all selected files (single entry for `write`; the matching file set for `pre-commit`).
- `$IRONLINT_ROOT` — project root (the check's cwd).
- `$IRONLINT_EVENT` — `write` or `pre-commit`.
- `$IRONLINT_TMPFILE` — **write only**, set only when your `run` mentions it: an absolute path to a temp file holding the proposed content, placed beside `$IRONLINT_FILE` with the same extension and auto-cleaned. Use it for tools that need a real file on disk (Biome, ESLint file-mode, `tsc`, ruff) instead of stdin. Unset on `pre-commit` (files are already on disk at `$IRONLINT_FILES`).
- `$IRONLINT_BIN` — absolute path to the `ironlint` binary running the check, so a check can shell out to it without relying on `PATH` resolution. Falls back to the bare name `ironlint` if the path can't be determined.
- `$IRONLINT_PROPOSED_MANIFEST` — optional; absolute path to a tab-separated (`file_path<TAB>content_path`) manifest of sibling proposed files in the same atomic patch. Set by some harness adapters; **absent otherwise — don't depend on it.**
- **stdin** — proposed post-edit file content (`write`) or empty (`pre-commit`).

**Read proposed content from stdin, not from `$IRONLINT_FILE`.** On harnesses that gate before the write lands (e.g. codex, pi), the file on disk still holds the OLD content, so reading it misses the very change you mean to check. Use `$IRONLINT_FILE` to hand a tool a filename (e.g. a linter's `--stdin-filename`), never as the content source.

**The check runs in a scrubbed environment.** The child process inherits only an allowlist — `PATH`, `HOME`, `LANG`, `TZ`, `TMPDIR`, and any `LC_*` — plus the `IRONLINT_*` vars above. The agent's own credentials (`ANTHROPIC_API_KEY`, `GITHUB_TOKEN`, `NPM_TOKEN`, `AWS_*`, …) are **not** inherited. Don't write a check that assumes the parent's ambient credentials; use a file, credential helper, or secrets manager that the check can access explicitly when a tool requires credentials.

On block, the check's combined stdout+stderr becomes the message the agent sees, so make the command print why it blocked.

## Top-level config

A minimal `.ironlint.yml` is just `checks:`. Two optional top-level keys tune it:

- `extends:` — a list of relative paths to other config files. ironlint resolves them recursively (with cycle detection); **inherited checks fill gaps where the local config doesn't define them, and local checks win on collision.** `execution` inherits the same way (nearest ancestor's value when local sets none). Trust covers the whole `extends` closure — editing any extended file invalidates the fingerprint.

  ```yaml
  extends:
    - ../shared/.ironlint.yml
  checks: {}
  ```

- `execution.timeout_secs` — per-check wall-clock (default `30`). A check that exceeds it is killed and reported as InternalError, never a silent pass. Override at run time with the `IRONLINT_TIMEOUT` env var (seconds).

  ```yaml
  execution:
    timeout_secs: 60
  ```

## Check patterns

**Ban a pattern (grep, reads stdin).** With nonzero-blocks, `! grep` is the natural idiom — grep exits 0 on a match (which `!` flips to 1, blocking) and exits 1 when clean (which `!` flips to 0, passing):

```yaml
  no-console-log:
    files: ["src/**/*.ts", "src/**/*.tsx"]
    run: "! grep -nE 'console\\.log\\('"
```

**Wrap a linter (stdin).** Feed the proposed content to a linter. Most linters exit nonzero on findings, which blocks directly:

```yaml
  ruff-check:
    files: ["**/*.py"]
    run: "ruff check --quiet --stdin-filename \"$IRONLINT_FILE\" -"
```

**Wrap a file-oriented linter (temp file).** Tools that won't read stdin cleanly get a real path:

```yaml
  biome-check:
    files: ["src/**/*.{ts,tsx,js,jsx}"]
    run: "npx @biomejs/biome check \"$IRONLINT_TMPFILE\""
```

**Limitation:** `$IRONLINT_TMPFILE` has a synthetic name (`ironlint-tmp-…`), so tools that select behaviour by *filename glob* — e.g. ESLint `overrides` scoped to `*.test.ts`, or Biome `include`/`ignore` patterns — won't match it the way they'd match the real file. Language detection (by extension) and nearest-config resolution (the temp file sits beside `$IRONLINT_FILE`) do work. When a tool needs the real filename for its config, pass `$IRONLINT_FILE` for that and `$IRONLINT_TMPFILE` only for the content.

**Multi-step check.** Use `steps` when you want to run multiple commands in sequence — all must exit 0:

```yaml
  ts-quality:
    files: "src/**/*.ts"
    on: [pre-commit]
    steps:
      - name: typecheck
        run: "tsc --noEmit"
      - name: no-any
        run: "! grep -n ': any' $IRONLINT_FILES"
```

**Multi-line scripts.** Use a YAML block scalar so newlines survive — a plain or folded (`>`) scalar collapses them and can turn the whole script into one comment that silently passes:

```yaml
  guard:
    files: "*.rs"
    run: |
      grep -q 'FORBIDDEN' && exit 1
      exit 0
```

**Pre-commit check.** Runs once over the selected matching file set, receiving those paths through `$IRONLINT_FILES`:

```yaml
  no-secrets:
    files: "**/*"
    on: [pre-commit]
    run: "detect-secrets scan $IRONLINT_FILES"
```

## Lifecycle placement

Place by legitimacy, not by preference. A check belongs at `write` if a file
tripping it is *never* legitimate — banned APIs, secrets, forbidden markers,
debug macros. Catching those at write stops the agent before the pattern
spreads across files (commit-time rework is far more expensive). A check
belongs at `pre-commit` if a legitimately mid-construction file could trip it
— formatting, import resolution, whole-tree compilation, tests. Blocking
those at write punishes TDD and partial edits.

When a check moves to `pre-commit`, it can no longer read proposed content on
stdin — re-scope the command to `$IRONLINT_FILES` (the matched set, on disk).
Example — rustfmt moved to the floor:

```yaml
rustfmt:
  name: rustfmt check
  files: "**/*.rs"
  on: [pre-commit]
  run: |
    echo "$IRONLINT_FILES" | tr '\n' '\0' | xargs -0 rustfmt --check --color=never
```

Project-scoped adapter settings (`.claude/settings*.json`, `.codex/hooks.json`,
`.opencode/plugins/`) are ordinary repo paths — a normal check scoping them
covers file-tool edits to them too. Home-scoped settings
(`~/.claude/settings.json`) sit outside every repo glob; only the Bash gate
covers those.

## Disable a check for a file

Add `# ironlint-disable: <check-id>` anywhere in the file to suppress that check for the whole file:

```python
# ironlint-disable: no-console-log
console.log("debug only")
```

## Process

1. Read `.ironlint.yml` to see existing checks (if none exists, scaffold one with `ironlint init`).
2. Draft the check: `files` scope + a `run` command that exits nonzero to block.
3. Build two fixtures: a **dirty** file the check should block, and a **clean** one it should pass.
4. After adding the candidate and re-trusting the config, test each fixture by feeding its content on stdin and isolating the check:
   ```bash
   ironlint check --file dirty.py --content - --check ruff-check < dirty.py ; echo "dirty exit: $?"   # expect nonzero
   ironlint check --file clean.py --content - --check ruff-check < clean.py ; echo "clean exit: $?"   # expect 0
   ```
5. Verify the check exits nonzero on dirty input and 0 on clean input.
6. Keep the check when both fixtures produce the expected result. Revise or remove it otherwise.

## Test before write

Test every check against a fixture before relying on it. A check that doesn't exit nonzero on dirty input gives false confidence. A check that exits nonzero on clean input blocks every edit in scope.
