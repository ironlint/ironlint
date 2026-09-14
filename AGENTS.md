# AGENTS.md

Guidance for work in this repository.

## Start here

- [Current architecture](docs/architecture.md): implemented behavior and source map.
- [V1 contract](specs/2026-09-05-ironlint-v1-design.md): release requirements.
- [Active plan](plans/2026-09-05-ironlint-v1-implementation.md): resume block,
  current packet, dependencies, and evidence. Read only the relevant packet and
  contract sections before tracing code; do not load the whole documentation tree.

Keep these documents current when behavior or scope changes. There is one active
v1 plan and no historical planning archive. Do not revive removed roadmap items.

## Current implementation and target

IronLint is a Rust command-check evaluator for AI coding workflows. `version: 1`
policies use `run`, optional `files`, and `on: [accept]` or `[change, accept]`.
The versioned core/CLI, schema-7 output, bounded execution, local consent, and
read-only inspection are implemented. External acceptance and a live-verified
completed-edit feedback adapter remain open.

Unversioned configs, schema-6 verdicts, preview execution, gate-bash, and the old
installer/adapters still exist in the checkout. **V1 is a breaking release:**
backward compatibility, automatic config conversion, migration reports, and
coordinated rollback are out of scope. Their presence does not require preserving
old behavior in v1. Remove them through the active plan, after tracing retained
callers and safely removing owned hook registrations. Preserve unrelated user
settings and hooks. Do not claim planned integration behavior already exists.

`ironlint schema` prints the installed authoring guide; it currently covers both
formats. `init` still scaffolds the unversioned format. See the architecture's
installation section before editing either.

## V1 contracts

- `accept` runs every check once in check-ID order. `change` filters opted-in
  checks by known paths; unknown paths run all change checks. Bare globs such as
  `*.rs` match at any depth. Every check must include acceptance.
- Commands run via `sh -c` in the supplied root, with stdin closed. Only
  `IRONLINT_ROOT`, `IRONLINT_EVENT`, and `IRONLINT_BIN` are supplied as reserved
  variables. Retained environment, output caps, and deadlines are specified in
  the v1 contract. Paths are env values, never spliced into `run`.
- CLI exits: 0 successful evaluation (including empty change selection), 1
  config/input failure, 2 violation, 3 execution/incomplete error, 4 untrusted
  policy. Schema 7 distinguishes `pass`, `violation`, `error`, and `not_run`.
  An enforcing consumer requires a complete acceptance pass, not exit 0 alone.
- Trust is CLI execution consent, keyed by canonical config path and policy/script
  hash. Keep the core evaluator independent of consent. Read-only inspection never
  requires trust. Consent is not isolation or authority over repository acceptance.
- The acceptance integration must bind approved policy/evaluator and a complete
  result to the exact candidate, with publication authority inaccessible to checks.
- Config-less `gate-bash` currently returns 0 allow / 2 block; existing adapters
  fail closed on gate errors. Remove owned registrations before deleting this path.
- Binary name: `ironlint`. Commit `Cargo.lock`; build and test with `--locked`.

## Validation and working rules

- Bug fixes start with a failing regression, then the smallest shared root-cause
  fix and focused test. Trace direct callers before editing or deleting code.
- Request a separate agent review after an integrated implementation batch.
  Review concrete regressions; do not repeat broad reviews without new evidence.
- Rust source must meet at least 80% region coverage per file and cognitive
  complexity at most 15. Refactor rather than suppress the lints.
- Run one Cargo build/test/coverage operation per target directory at a time.
- Installation/adapter tests use a temporary home/config directory. Never exercise
  them against the developer's live trust store or hook installation.
- Remove transient build/scratch artifacts produced by the task; keep the normal
  iterative `target/`. Mutation testing is optional investigation, not a CI gate.
- Keep provenance-stamped adapter fixtures and capture-pending declarations intact
  while their suites are active. Run `adapter-drift-audit` when a harness contract
  changes. Existing Codex captures cover pre-write add/update; the other three
  harnesses remain capture-pending. V1 needs one live-verified feedback adapter.

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all --check
bash scripts/ci-coverage.sh
bash scripts/ci-adapters.sh
```

The adapter script needs isolated `XDG_CONFIG_HOME` when run locally. Full release
validation also needs the live evidence in the plan; green unit tests cannot prove
repository enforcement. Publishing requires task authorization.

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
