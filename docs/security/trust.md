# Execution consent

Policy commands run with the invoking account's filesystem permissions. Review
the policy and managed scripts, then grant consent explicitly:

```sh
ironlint validate --config /work/policy.yml
ironlint trust --config /work/policy.yml
ironlint check --config /work/policy.yml --root /work/candidate --event accept
```

Consent is stored outside the repository at the XDG config location
(`~/.config/ironlint/trust.json` by default), keyed by canonical policy path.
It covers the policy bytes and content below `.ironlint/scripts/`. Changed
approved inputs need renewed consent. Commands can invoke files elsewhere in
the project, so review those files too; they are not part of this managed trust
surface.

`ironlint check` evaluates the exact policy bytes it verified rather than
re-reading the policy path, and re-verifies the policy and every managed script
before each check and once after the run. If they change mid-run, the remaining
checks are `not_run` with reason `policy_changed`, the verdict is `error`, and
the process exits 3 — a partial pass never looks like acceptance. A check that
rewrites a managed script and then runs it is not a supported pattern; review
the new content and grant consent again.

`ironlint init` records consent for the baseline policy bytes it writes or
recognizes, so a retry after a failed consent write completes instead of
skipping consent, and a config you edited is never modified or blessed by
`init`.

`ironlint check` enforces consent. Read-only commands such as `validate`,
`explain`, and `show-resolved-config` can inspect a policy without running it.
An untrusted policy returns exit 4; invalid or unverifiable inputs can return
exit 1.

Library evaluation can use a validated immutable `PolicySnapshot` without a
local consent store. `evaluate_v1_snapshot` takes a typed event and uses the same
selection, execution limits, and drift checks as the CLI. The existing
`evaluate_v1(&ApprovedPolicy, ...)` API forwards to it. CLI execution still
requires consent; a library result does not grant publication authority.

Snapshot loading captures the policy bytes and managed-script identity once.
Direct and worktree consent hashes describe that same capture; verification
streams script bytes using a fixed buffer and compares ordered digests. Consent
formats and hash framing are unchanged. The execution deadline includes repeated
verification, while initial capture and consent lookup occur before execution.
Managed script entries must be regular files or ordinary directories; symlinks
and special entries fail verification. A file that changes size or identity
during a streamed read also fails instead of approving a partial capture.

Consent is not a sandbox. Commands can still read and modify what your account
can access, and changes outside the managed policy surface are not an approval
signal. If a CI system or hosting service uses the result to make a decision,
protect its credentials and bind the result to the change it evaluated.
