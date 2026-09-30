# AI-tool integrations

IronLint works directly from the command line. Adapters are optional bridges
that let a coding tool call IronLint while you work. Always run
`ironlint check --event accept` in the workflow that decides a change is
complete; an adapter does not replace that full check.

## Available adapter

Pi runs after a supported edit and shows feedback while leaving the edit in
place.

| Coding tool | What the adapter does | Details |
| --- | --- | --- |
| Pi | Feedback after selected edits, using the policy shown in this guide | [Setup](../../adapters/pi/README.md) |

The host tool controls which edits it sends to an adapter. Pi feedback is not
an acceptance decision, so retain a full acceptance check in the workflow that
decides work is complete.

## Install and remove an adapter

Choose a tool explicitly:

```sh
ironlint init --harness pi
ironlint init --uninstall --harness pi
```

In an interactive terminal, `init` prints the installation plan and asks for a
line-based confirmation. Detected Pi is selected by default; installing Pi
when it was not detected requires an affirmative answer. EOF or cancellation
declines. In a noninteractive run, an explicit `--harness` selection is
treated as confirmation; use `--dry-run` to preview it instead. Automatic
noninteractive installation of detected Pi needs `--yes`; installing undetected
Pi requires explicit `--harness pi`.

Removal deletes only files IronLint can identify as its own. If an adapter file
was edited, replaced, or shares its directory with your files, IronLint leaves
the affected content for you to review. Your policy and execution-consent record are not
removed. Use `--git-hook` only when you explicitly want the optional pre-commit
acceptance hook; uninstall removes its owned marked section while preserving
an existing user hook.

Older IronLint releases could install Claude Code, Codex, and OpenCode
adapters. New installation is disabled. To remove owned legacy installations,
run `ironlint init --uninstall --harness all`; cleanup covers both local and
global adapter locations. Edited or unrecognized files are left for manual
review, and any cleanup error makes the command fail.

Interactive automatic uninstall lists owned registrations and confirms their
cleanup. Pi uses the selected scope: project by default, or global with `--global`.
Legacy cleanup inspects both local and global locations. Use explicit flags for
scripts; `--dry-run` previews without prompting or writing, and `--yes` confirms
the printed plan.

Run `ironlint doctor` to inspect the local policy, shell, consent, and visible
adapter files. It can report what exists on disk, but it cannot prove a host
tool will send every edit through an adapter.
