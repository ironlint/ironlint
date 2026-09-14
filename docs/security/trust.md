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
Hashing covers config bytes and managed `.ironlint/scripts/` content. The
currently retained unversioned path also resolves inheritance for hashing.
Changed approved inputs need renewed consent; arbitrary transitive dependencies
are not all hashed.

The CLI `check` enforces consent. Read-only inspection can examine unapproved
policies. The core evaluator remains independent of the store. Untrusted input
returns exit 4; unverifiable inputs can return exit 1. Neither authorizes acceptance.

An external owner must select approved policy/evaluator inputs and isolate
publication credentials from candidate execution. Candidate flags/environment
cannot grant trusted CI status. Consent is not a sandbox or authority boundary
against another process sharing the account.

## Existing Bash guardrail

The current gate-bash recognizes trust commands and protected-surface mutations
through integrated shell tools. Its coverage has indirection limits. It is slated
for removal with owned hook registrations and does not establish external acceptance.
