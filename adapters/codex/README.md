# IronLint — Codex adapter

Package 0.1.0 provides post-edit feedback and fresh full acceptance at Stop.
It uses the v1 IronLint CLI, not the removed pre-edit or LLM evaluator paths.

## Install

Install IronLint 1.1.0+, Python 3.9+, and the adapter package on Linux/macOS.
Review and trust your project's .ironlint.yml policy first. Select extracted
package files, or this repository's adapters directory:

```sh
export IRONLINT_ADAPTERS_ROOT=/absolute/path/to/adapters
ironlint init --harness codex
ironlint doctor
```

Project registration uses `.codex/hooks.json`. `--global` selects the harness's
user-level settings. Native hook artifacts are shared under
`$XDG_CONFIG_HOME/ironlint/adapters/codex` (falling back to ~/.config).
Reinstall updates unmodified owned files and migrates owned legacy registrations
in the current local/global scopes. Edited or foreign content is preserved.

Restart Codex and review/trust these non-managed hooks when prompted.
IronLint installation does not grant Codex hook trust.

## Behavior

PostToolUse observes apply_patch (including Edit/Write matcher aliases). Known changed paths filter change checks;
unknown mutation paths run all change checks. Codex patch moves include both
endpoints and deletes retain their deleted path. Failures and evaluator errors
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

The contract was checked against the [official hooks reference](https://learn.chatgpt.com/docs/hooks) on
2026-10-02. Local installed CLI observation: 0.159.1; this is an observed
version, not a minimum-version or live-model qualification claim. Synthetic
fixtures plus real IronLint integration tests cover red/repair, missing consent,
unknown paths, fresh acceptance, packaging, and bounded execution. Live model
sessions and Windows descendant cleanup are not qualified by this suite.

```sh
ironlint init --uninstall --harness codex
```

Removal checks owned registrations in both scopes and preserves your edits.
The [package guide](../README.md) describes independent builds and releases.
