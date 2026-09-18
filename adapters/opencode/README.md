# OpenCode adapter cleanup

IronLint v1 does not install the former OpenCode plugin. This directory is
retained only so IronLint can recognize and safely remove files it installed
with an earlier release.

```sh
ironlint init --uninstall --harness opencode
```

Removal deletes only recognized owned artifacts. Edited or unrelated user
files are left in place for manual review. Use the
[v1 command-line workflow](../../docs/getting-started.md) for current policies.
