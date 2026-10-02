# IronLint — Claude Code adapter

Package 0.1.0 provides post-edit feedback and fresh full acceptance at Stop.
It uses the v1 IronLint CLI, not the removed pre-edit or LLM evaluator paths.

## Install

Install IronLint 1.1.0+, Python 3.9+, and the adapter package on Linux/macOS.
Review and trust your project's .ironlint.yml policy first.

### Marketplace package

The repository catalog selects `ironlint-claude-code-plugin-0.1.0.zip`, pinned
with SHA-256. Archive sources require Claude Code 2.1.224+; see the
[marketplace hosting reference](https://code.claude.com/docs/en/plugins/host-marketplace#serve-users-who-have-no-git-host-account).
The release URL is prepared but unpublished. After the archive and catalog are
published, install with:

```sh
claude plugin marketplace add ironlint/ironlint
claude plugin install ironlint-claude-code@ironlint-marketplace
```

The catalog retains the marketplace name and maps the former `ironlint` entry
to `ironlint-claude-code`. Existing users should update the marketplace and
install the renamed entry. The package includes both Python helpers and the
authoring skill inside the plugin directory. Installing the source directory
directly through a marketplace omits its shared dependencies.
The plugin ZIP places plugin content at its root. The separate installer ZIP
includes the harness and shared directories needed by `ironlint init`.

Archive marketplace installation has not been tested with a supported client.
The observed local CLI is 2.1.207, below the archive-source minimum. The isolated
package-cache regression checks that both hooks run after copying only the
packaged plugin directory; it does not test a marketplace download or a live
model session.

### Local registration

For clients below the archive-source minimum, use the local registration path.
Select extracted package files, or this repository's adapters directory:

```sh
export IRONLINT_ADAPTERS_ROOT=/absolute/path/to/adapters
ironlint init --harness claude-code
ironlint doctor
```

Project registration uses `.claude/settings.local.json`. `--global` selects the harness's
user-level settings. Native hook artifacts are shared under
`$XDG_CONFIG_HOME/ironlint/adapters/claude-code` (falling back to ~/.config).
Reinstall updates unmodified owned files and migrates owned legacy registrations
in the current local/global scopes. Edited or foreign content is preserved.

Reload Claude Code after installation so it reads the hook configuration.
Choose either marketplace installation or local registration for a project;
using both registers duplicate hooks.

## Behavior

PostToolUse observes Write, Edit, MultiEdit, and NotebookEdit. Known changed paths
filter change checks; unknown mutation paths run all change checks. Failures and evaluator errors
are added as structured additionalContext; the completed edit stays in place.
Successful feedback remains silent, including an empty change selection.

Stop inspects the complete required check-ID set without executing checks, then
runs fresh full acceptance and strictly validates schema 7, outcome/exit code,
check IDs, errors, unrun checks, and truncation. A violation requests one repair
continuation with diagnostics and a reproduction command. A repeated violation
with stop_hook_active reports incomplete acceptance and stops automatic repair
churn. Errors, missing consent/binaries/policies, and declared background work
also report incomplete acceptance; the hook never automatically grants trust,
alters the policy, runs a diagnostic-provided repair command, or reuses a pass.

Each hook has a 600-second total deadline across inspection and evaluation,
an 8 MiB input/stdout cap, 64 KiB retained stderr, and a 2-second cleanup grace.
The native timeout is 610 seconds. Cancellation closes the evaluator's opt-in
stdin channel before stopping its owned POSIX process group. Command stdin
inside the evaluator stays closed. IRONLINT_BIN can select an owner-configured
binary outside hook payloads; the default is ironlint on PATH.

## Limits and compatibility

This is a local workflow aid. It evaluates the mutable hook cwd and does not
bind success to an immutable candidate, isolate rule execution, or establish
merge/publication authority. The host owns hook delivery, continuation caps,
other hooks, and interruption. Shell/MCP/unsupported writes can miss early
feedback; Stop still evaluates every required check. Other projects with old
local PreToolUse registrations need their own reinstall. Hook errors or another
Stop hook can override continuation, so retain an independent acceptance gate
where your workflow requires one.

The contract was checked against the [official hooks reference](https://code.claude.com/docs/en/hooks) on
2026-10-02. Local installed CLI observation: 2.1.207; this is an observed
version, not a minimum-version or live-model qualification claim. Synthetic
fixtures plus real IronLint integration tests cover red/repair, missing consent,
unknown paths, fresh acceptance, packaging, and bounded execution. Live model
sessions and Windows descendant cleanup are not qualified by this suite.

```sh
ironlint init --uninstall --harness claude-code
```

Removal checks owned registrations in both scopes and preserves your edits.
The [package guide](../README.md) describes independent builds and releases.
