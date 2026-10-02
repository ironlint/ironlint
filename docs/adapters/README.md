# AI-tool integrations

IronLint runs directly from the CLI. Optional adapter packages connect it to
coding tools and can be updated independently of the evaluator.

| Coding tool | Integration | Details |
| --- | --- | --- |
| Pi | Post-edit feedback; separate controlled completion runner | [Setup](../../adapters/pi/README.md) |
| Codex | Post-edit feedback and full acceptance at Stop | [Setup](../../adapters/codex/README.md) |
| Claude Code | Post-edit feedback and full acceptance at Stop | [Setup](../../adapters/claude-code/README.md) |

Codex and Claude Code native hooks provide local feedback and request repair.
They do not bind a verdict to an immutable candidate or establish publication
permission. Keep full acceptance in the workflow that decides work is complete.
Pi's explicit controlled runner has its own documented candidate contract.

## Install packages and register hooks

Adapter files ship separately from the Rust binary. Extract an adapter archive
and select its root (the directory containing `codex/`, `claude-code/`, or `pi/`
and `shared/`), or use this checkout's `adapters/` directory:

```sh
export IRONLINT_ADAPTERS_ROOT=/absolute/path/to/adapters
ironlint init --harness codex
ironlint init --harness claude-code
ironlint doctor
```

Installed binaries also discover `adapters/` beside their executable. Development
builds fall back to this checkout; `cargo install` and standalone release binaries
need separately installed package files. Installation copies owned hook files and
skills, so running an installed hook does not depend on the package checkout.
Updating package files and rerunning `init` updates unmodified owned artifacts;
no Rust rebuild is needed. The [package guide](../../adapters/README.md) covers
independent versioning, archives, and compatible runtimes.

Review and trust the policy before execution. Codex additionally requires review
and trust of non-managed hooks in Codex. `init` never grants that harness consent.
Native hooks require Python 3.9+ and POSIX process handling on Linux/macOS.

Interactive setup prints a plan and asks for line-based confirmation. Detected
harnesses are selected by default; Pi is offered when none is detected. An
explicit noninteractive `--harness` selection is confirmation; automatic setup
needs `--yes`. Use `--dry-run` for a read-only preview.

## Ownership, migration, and removal

```sh
ironlint init --uninstall --harness codex
ironlint init --uninstall --harness claude-code
ironlint init --uninstall --harness all
```

Installation migrates owned old PreToolUse registrations to current post-edit
and Stop registrations in the current project's local settings and global
settings when replacing shared native hook files. Foreign handlers remain.
Other projects' old local registrations require their own reinstall. Native
adapter removal checks both scopes because they share the same owned files.
Pi uses the selected scope; OpenCode remains available for owned cleanup only.

Removal requires no package sources. It preserves edited, foreign, symlinked,
or unrecognized content and reports incomplete cleanup. Policies and execution
consent remain in place. The optional Git pre-commit hook is installed only with
`--git-hook`; it retains its separate documented acceptance contract.

Doctor inspects both scopes without executing checks. It reports missing files,
edited artifacts, old packages/registrations, and missing required Stop hooks.
When source packages are unavailable it can still inspect ownership and missing
registrations, but cannot establish whether installed package bytes are current.
It cannot prove live harness delivery or enforce every write route.
