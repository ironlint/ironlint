# Secret Scanner Corpus Extraction Design

**Status:** Approved  
**Date:** 2026-07-30

## Decision

IronLint will not ship first-party secret scanning. The feature is cancelled
in this project and may later become a separate standalone CLI.

The existing implementation on `codex/first-party-secret-scan` will be
preserved in `/Users/chrisarter/Documents/projects/ironlint-secrets` as a
foundational code corpus. It is intentionally parked, not promised as working
software.

## Transfer

Export the feature worktree's complete tracked tree plus its two untracked
review-plan files. Exclude linked-worktree Git metadata, ignored Rust build
output, and ignored Superpowers scratch data:

- `.git`
- `target/`
- `.superpowers/`

Add `ORIGIN.md` to the destination with the source repository, branch, commit,
cancellation decision, and intended future use. Initialize the destination as
an independent Git repository and commit the preserved corpus.

## IronLint documentation

Keep the existing implementation plan as a historical record, but mark it
cancelled and point to the parked corpus. Update the backlog entry so it no
longer presents the scanner as ready IronLint work.

External secret-scanner examples remain valid because IronLint still supports
ordinary user-authored checks that invoke separate tools.

## Verification and cleanup

Before deleting the source worktree or branch:

1. compare the exported source file set and content against the worktree,
   accounting only for the new destination metadata;
2. confirm the destination commit contains both formerly untracked plans;
3. confirm `main` contains no first-party scanner implementation;
4. confirm documentation calls the IronLint feature cancelled and identifies
   the standalone CLI direction.

Only after those checks pass, remove the linked worktree and its local feature
branch.
