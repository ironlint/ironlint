# Security review findings — 2026-10-01

Handoff for follow-up work on branch `codex/harness-compliance-implementation` at
`3db5804`. This was a targeted review of the optional Git hook, Pi controlled
completion, and locked dependencies. No product code was changed during the
review. The pre-existing local edit to `.codex/hooks.json` was left untouched.

## Remediation

All three reproduced cases have focused regression coverage. The optional hook
now refuses mismatched staged/working regular files instead of attempting a
new staged-tree execution mode. It compares raw bytes and executable status
before and after acceptance, requires a staged and working policy, rejects
reappearing staged deletions, and checks that the index has not changed. This
also covers clean filters and skip-worktree/assume-unchanged flags. Symlink target
bytes must match, and submodules fail closed. Untracked/ignored dependencies,
external symlink targets, concurrent same-account
tampering, and hook bypass remain documented local-workflow limits.

Pi's shared tool path guard now rejects multiply linked regular files before
read, edit, and write operations. Candidate path validation rejects `.git`
components case-insensitively, including declared ignored inputs. Filesystem
isolation and descriptor-relative operations remain outside this scoped fix.

The reproduction details below describe the original code. Existing hook users
must run `ironlint init --git-hook` to install the updated guard.

Validation of the remediation:

- Full Rust suite: 524 tests passed in a clean temporary source copy. The normal
  checkout contains pre-existing empty retired adapter hook directories that
  trigger an unrelated registry test; those directories were preserved.
- Final installed-hook suites: 17 tests passed, including partial staging,
  policy removal, clean filters, index flags, changes during acceptance, staged
  deletion, file-to-directory replacement, and unusual file/symlink names.
- Rust coverage: 94.07% regions overall; every file met the 80% gate. Clippy
  with warnings denied, formatting, and Pi TypeScript checks passed.
- All 63 Pi tests passed with `--test-concurrency=1`; the default parallel
  adapter run repeatedly hit the existing two-second process-start timeout in
  `D7 ordinary cancellation reaps the owned child and closes descendant pipes`.
  That test also passed in isolation. No unrelated timeout behavior was changed.
- Acceptance-consumer tests, check recipes, and the local Docker feature suite
  passed. The final hook guard also passed a Linux shell smoke test for matching
  symlinks (including dash/newline names) and rejection of a byte mismatch.
- Separate agent review found file-to-directory and leading-dash symlink
  compatibility issues; both were fixed with regression coverage.

## 1. Optional pre-commit hook checks different bytes than Git commits

**Severity:** Medium. **Status:** Reproduced; already acknowledged as a v1
limitation in [the contract](../../specs/2026-09-05-ironlint-v1-design.md#L388).

The generated hook runs `ironlint check --event accept` against the working
tree, then lets Git commit the index. See
[`hook_block`](../../crates/ironlint-cli/src/commands/init/git_hook.rs#L184-L201),
especially the `--root "$ROOT"` invocation. It also exits successfully when
the working-tree `.ironlint.yml` is absent. Neither decision establishes that
the staged tree passed acceptance.

**Isolated reproduction:** In a temporary Git repository, install the hook and
trust a policy whose check is `grep -qx clean rule.txt`. Write `bad` to
`rule.txt`, stage it, then replace only the working-tree copy with `clean`.
The actual installed hook prints `no_bad: pass` and allows `git commit`;
`git show HEAD:rule.txt` returns `bad`. The test used a temporary
`XDG_CONFIG_HOME`, so it did not touch the developer's trust store.

**Patch direction:** If the hook is meant to attest to commit content,
materialize the exact staged tree in a private directory and run full
acceptance against it. Bind the policy and check dependencies to that tree or
to owner-controlled inputs. Fail closed when a required staged policy is
absent. Keep the documented distinction between a local convenience hook and
an independently enforced repository acceptance boundary; `--no-verify` can
still skip a local Git hook.

## 2. Pi tool path guard accepts hardlinks to protected files

**Severity:** Medium, conditional on a hardlink already being present in the
workspace. **Status:** Reproduced with an isolated fixture.

[`confined`](../../adapters/pi/src/sdk.ts#L36-L50) rejects paths outside the
workspace, `.git` path components, and symlinks. The
[`writeFile` operation](../../adapters/pi/src/sdk.ts#L59-L66) then opens the
allowed path. A regular file in the workspace can be a hardlink to a protected
file. In a temporary fixture, a hardlink named `innocent.txt` to
`.git/config` passed `confined`; writing through the returned path changed
`.git/config`.

This is not a hardlink that an ordinary Git checkout creates. It matters when
the controlled session starts in a workspace containing untrusted filesystem
entries. The local runner's documented same-account limit remains relevant;
this finding does not imply tamper-resistant enforcement was promised.

**Patch direction:** Reject multiply linked regular files before model read,
edit, or write operations, including at session startup if practical. Path
checks followed by a separate open also permit symlink-swap races; use
descriptor-relative, no-follow operations or an isolated workspace for a
stronger file boundary. Add a regression with a hardlink to `.git/config`.

## 3. Case-insensitive `.GIT` alias can enter candidate artifacts

**Severity:** Low; requires the caller to supply the affected
`--ignored-input` value. **Status:** Reproduced on this case-insensitive macOS
filesystem.

[`safePath`](../../adapters/pi/src/candidate.ts#L36-L40) rejects a path
component equal to `.git` but accepts `.GIT`. On this filesystem,
`captureCandidate(root, store, [".GIT/config"])` copied the repository's
`.git/config` into the retained candidate artifact. This contradicts the
candidate capture contract's stated `.git` exclusion. Git itself rejected an
attempt to add `.GIT/config` to the index in the same fixture, so the observed
route was the explicit ignored-input option, not an ordinary tracked file.

**Patch direction:** Reject `.git` components case-insensitively, as the Pi
tool guard already does. Add a case-insensitive filesystem regression for
declared ignored inputs, and consider validating the final resolved path before
copying it.

## Dependency check and limits

- `npm audit --package-lock-only --audit-level=low` in `adapters/pi` returned
  `found 0 vulnerabilities`.
- A versioned [OSV batch query](https://google.github.io/osv.dev/post-v1-querybatch/)
  for all 145 packages in `Cargo.lock` returned no advisory matches.
- These results cover known advisories at review time, not unknown flaws or
  whether every dependency is safe in this program's use.
- The review did not run the full test suite or a live Pi model session. The
  three isolated reproductions above used temporary directories and were
  removed afterward.
