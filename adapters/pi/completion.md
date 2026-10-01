# Controlled Pi completion (qualified local path)

`ironlint-pi-complete` is an explicit Pi SDK entry point. Ordinary Pi sessions and
the installed feedback extension keep their existing feedback-only behavior.
The controller owns the task result: model text is held until a fresh full
`accept` evaluation passes for the returned candidate artifact.

## Runtime and launch

The supported SDK version is exactly `@earendil-works/pi-coding-agent` **0.87.1**
(source commit [`f07218c4d4bbc12bef056a7058c3dd49dfe41abe`](https://github.com/earendil-works/pi/tree/f07218c4d4bbc12bef056a7058c3dd49dfe41abe)).
The runner checks this version at startup and returns `incomplete` for a missing
or different version. Node.js 22.6+ is required for the TypeScript launcher.
The tested platform is macOS; descendant process-group behavior is qualified on
Unix. Other platforms require their own qualification.

Install the Pi package dependencies, then invoke from a caller-controlled Git
worktree. The owner must provide an absolute, reviewed policy path, the absolute
IronLint binary path, the exact required check IDs, and an external artifact
directory. Trust the policy separately in the caller's consent store.

```sh
cd adapters/pi && npm ci
./bin/ironlint-pi-complete \
  --root /absolute/candidate-repo \
  --policy /absolute/owner/policy.yml \
  --binary /absolute/ironlint \
  --artifacts /absolute/owner/artifacts \
  --checks fmt,test \
  --task 'Implement the requested change'
```

Optional `--ignored-input path` can repeat for required ignored files. Optional
`--max-repair-turns N` and `--deadline-seconds N` override defaults of three
repair turns and 900 seconds. `--agent-dir` selects Pi auth/model files. The
caller provides credentials through Pi's normal SDK configuration; the runner
reads only `defaultProvider` and `defaultModel` from that directory's global
`settings.json` and does not load candidate project settings. It does not create
consent or install a model. A new launch after interruption is
a fresh attempt and always evaluates again.

The single JSON line on stdout has `status` (`complete` or `incomplete`),
`diagnostics`, `repairTurns`, and, only on success, `modelText` and `candidate`
(`identity`, `tree`, `artifact`). Exit 0 means complete; exit 3 means incomplete.
The caller owns the returned artifact and must preserve it until downstream
acceptance or cleanup. A local commit or model assertion cannot set this status.
SIGINT/SIGTERM request cancellation and return incomplete. Evaluator processes
are reaped through the subprocess owner.
If a Pi tool has not settled within the two-second abort grace, diagnostics say
that cleanup is unconfirmed and writes may still be in flight. The caller must
not reuse that workspace until the work has stopped or been isolated.

## Candidate and evaluation

The first candidate type is a Git repository top level. Capture copies the
current working bytes of tracked and nonignored untracked regular files,
including dirty and partially staged files; deleted files are absent and
executable bits are retained. Declared ignored inputs join the manifest and
identity. It neither stages nor commits. Symlinks, submodules, special files,
missing declared inputs, nested repository roots, and an artifact store inside
the source root are rejected. `.git` is excluded. Source, evaluation tree, and
retained artifact manifests are compared around each evaluation. Scratch build
output goes to `TMPDIR` outside the evaluated tree. A check that persistently
changes evaluated source invalidates a pass.

The controller waits for Pi `agent_settled`, `isIdle`, zero queued messages, and
a final assistant `stop` before capture. It provides confined read/edit/write
tools and loads no candidate extension or agent instruction files. It supplies
no Bash tool; edits from other processes are detected by manifest comparison.
Each attempt runs **all** owner-selected acceptance checks, regardless of change
filters or early feedback. The strict schema-7 consumer checks process status,
exact IDs, every command outcome/status, and absence of errors and unrun checks.
Only a complete pass yields the artifact. A rule violation yields bounded repair
feedback and another fresh attempt; evaluator errors, missing consent, timeout,
malformed output, and budget exhaustion yield incomplete.

The owner policy, evaluator binary, artifact store, and controller code must
remain outside candidate control. Required scripts and enforcement dependencies
must come from owner-approved sources. If an owner policy invokes scripts in the
candidate tree, those scripts are candidate-controlled and the resulting check
does not establish an independent rule. Declare any ignored input needed by a
check; arbitrary undisclosed shell dependencies cannot be inferred.

This is a **local workflow completion** guarantee under same-account assumptions.
Manifest comparison detects persistent drift, but a same-account process can
tamper with the controller, trust store, binary, or source during evaluation.
For the v1 contract's stronger enforced-acceptance claim, the caller must supply
an isolated workspace and independent owner-controlled policy, evaluator,
controller, and result recording. This runner alone is not that security boundary.

## Qualification

`npm run typecheck`, `IRONLINT_TEST_BIN=/absolute/ironlint npm test`,
`bash scripts/test-verify-acceptance.sh`, and `scripts/test-check-recipes.sh`
exercise the integration. `test/compliance/pi-runtime.test.ts` uses the actual
0.87.1 SDK with a deterministic provider to observe settlement, queued follow-up,
and abort. `test/compliance/complete-real.test.ts` uses that SDK and the real
IronLint CLI to deny a violation and accept a repair. Synthetic extension event
fixtures remain feedback tests. Live model repair utility measurements are not
yet available; see the qualification record before claiming release readiness.

Pinned upstream sources: [SDK](https://github.com/earendil-works/pi/blob/f07218c4d4bbc12bef056a7058c3dd49dfe41abe/packages/coding-agent/docs/sdk.md),
[extensions](https://github.com/earendil-works/pi/blob/f07218c4d4bbc12bef056a7058c3dd49dfe41abe/packages/coding-agent/docs/extensions.md).
