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

`ironlint check` enforces consent. Read-only commands such as `validate`,
`explain`, and `show-resolved-config` can inspect a policy without running it.
An untrusted policy returns exit 4; invalid or unverifiable inputs can return
exit 1.

Consent is not a sandbox. Commands can still read and modify what your account
can access, and changes outside the managed policy surface are not an approval
signal. If a CI system or hosting service uses the result to make a decision,
protect its credentials and bind the result to the change it evaluated.
