# Secret Scanner Corpus Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve the cancelled IronLint secret-scanner implementation in an
independent repository, record its origin, and remove the abandoned worktree
from IronLint.

**Architecture:** Clone only the feature branch into the empty destination so
Git preserves its history without linked-worktree metadata or ignored build
artifacts. Copy the two untracked review plans, add one origin note, verify the
destination, then remove the source worktree and branch. Main receives only
cancellation documentation; no scanner code changes are needed because the
feature never merged.

**Tech Stack:** Git, Markdown, Rust repository artifacts

## Global Constraints

- Preserve feature commit `7383671` and both untracked review-plan files.
- Do not copy `.git`, `target/`, or `.superpowers/` from the linked worktree.
- Do not remove the source worktree or branch until destination verification
  passes.
- Preserve all unrelated uncommitted changes on IronLint `main`.
- Do not remove external secret-scanner recipe examples.

---

### Task 1: Preserve the feature corpus

**Files:**
- Create repository: `/Users/chrisarter/Documents/projects/ironlint-secrets`
- Copy: `docs/superpowers/plans/2026-07-24-secret-scan-review-remediation.md`
- Copy: `plans/2026-07-20-secret-scan-review-fixes.md`
- Create: `/Users/chrisarter/Documents/projects/ironlint-secrets/ORIGIN.md`

**Interfaces:**
- Consumes: local branch `codex/first-party-secret-scan` at `7383671`
- Produces: independent `ironlint-secrets` Git repository on `main`

- [ ] **Step 1: Clone the feature branch into the empty destination**

Run:

```bash
rtk git clone --single-branch --branch codex/first-party-secret-scan \
  /Users/chrisarter/Documents/projects/ironlint \
  /Users/chrisarter/Documents/projects/ironlint-secrets
```

Expected: the destination checks out commit `7383671`, has no `target/`, and
contains the scanner source, tests, docs, and feature history.

- [ ] **Step 2: Copy the untracked review plans**

Copy these files byte-for-byte from the linked worktree into the matching
destination paths:

```text
docs/superpowers/plans/2026-07-24-secret-scan-review-remediation.md
plans/2026-07-20-secret-scan-review-fixes.md
```

- [ ] **Step 3: Add the origin note**

Create `ORIGIN.md` stating:

- the corpus came from IronLint;
- the source branch and commit were
  `codex/first-party-secret-scan` / `7383671`;
- IronLint cancelled first-party secret scanning on 2026-07-30;
- future work belongs in a separate standalone CLI;
- the repository is parked source material and is not promised to build or
  ship as-is.

- [ ] **Step 4: Make the repository independent**

Run:

```bash
rtk git remote remove origin
rtk git branch -M main
```

- [ ] **Step 5: Verify and commit the corpus**

Verify:

```bash
rtk git merge-base --is-ancestor 7383671 HEAD
rtk git status --short
rtk test ! -e target
rtk test -f ORIGIN.md
rtk test -f docs/superpowers/plans/2026-07-24-secret-scan-review-remediation.md
rtk test -f plans/2026-07-20-secret-scan-review-fixes.md
```

Commit only the origin note and copied plans:

```bash
rtk git add ORIGIN.md \
  docs/superpowers/plans/2026-07-24-secret-scan-review-remediation.md \
  plans/2026-07-20-secret-scan-review-fixes.md
rtk git commit -m "docs: park IronLint secret scanner corpus"
```

### Task 2: Record the cancellation on IronLint main

**Files:**
- Modify: `plans/2026-07-15-first-party-secret-scan.md`
- Modify: `plans/BACKLOG.md`

**Interfaces:**
- Consumes: approved extraction design and destination path
- Produces: planning records that no longer present the scanner as IronLint
  roadmap work

- [ ] **Step 1: Mark the implementation plan cancelled**

Add a prominent notice below the title with the cancellation date, standalone
CLI decision, and parked-corpus path. Keep the original plan below the notice
as historical context.

- [ ] **Step 2: Remove the feature from the active backlog**

Delete the ready plan reference from `plans/BACKLOG.md`; cancelled work is not
forward-looking backlog.

- [ ] **Step 3: Verify documentation**

Run:

```bash
rtk rg -n "cancelled|standalone CLI|ironlint-secrets" \
  plans/2026-07-15-first-party-secret-scan.md
rtk rg -n "first-party-secret-scan|First-party.*secret scanner" plans/BACKLOG.md
```

Expected: the plan contains all three cancellation markers; the backlog search
returns no matches.

### Task 3: Remove the abandoned IronLint worktree

**Files:**
- Remove worktree:
  `/Users/chrisarter/Documents/projects/ironlint/.worktrees/first-party-secret-scan`
- Delete local branch: `codex/first-party-secret-scan`

**Interfaces:**
- Consumes: verified destination repository
- Produces: no abandoned scanner worktree or local branch in IronLint

- [ ] **Step 1: Recheck the destination**

Run:

```bash
rtk git status --short --branch
rtk git log --oneline --decorate -2
rtk git show --stat --oneline 7383671
```

Expected: clean `main`, a corpus commit above the preserved feature history,
and source commit `7383671` remains reachable.

- [ ] **Step 2: Remove the source worktree and branch**

Run from IronLint:

```bash
rtk git worktree remove --force \
  /Users/chrisarter/Documents/projects/ironlint/.worktrees/first-party-secret-scan
rtk git branch -D codex/first-party-secret-scan
```

- [ ] **Step 3: Verify IronLint state**

Run:

```bash
rtk git worktree list --porcelain
rtk git branch --list codex/first-party-secret-scan
rtk rg -n "SecretScanner|BuiltinInvocation::Secrets|ironlint scan secrets" crates
rtk git status --short --branch
```

Expected: the removed worktree and branch are absent, the source scan returns
no matches, and all unrelated pre-existing main changes remain.
