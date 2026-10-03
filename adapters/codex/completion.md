# Explicit Codex completion

`bin/ironlint-codex-complete` owns a local task result separately from ordinary
Codex sessions and Stop hooks. Use the extracted adapter package or this source
checkout with Node.js 22.6+ on Linux/macOS. No extra hook is installed. Exact
runtime support and observed replay results are recorded in
[completion evidence](completion-evidence.md); other versions return incomplete.

```sh
./adapters/codex/bin/ironlint-codex-complete \
  --root /absolute/candidate-repo \
  --policy /absolute/owner/policy.yml \
  --binary /absolute/ironlint \
  --artifacts /absolute/owner/artifacts \
  --runtime /absolute/codex \
  --runtime-config /absolute/owner/codex.json \
  --checks fmt,test \
  --task 'Implement the requested change'
```

Policy, evaluator, runtime/configuration, controller, and artifact store must be
owner-controlled and outside the candidate root. Review and trust the policy
separately; the launcher never grants consent. Runtime configuration selects
the model/provider and credentials explicitly, with isolated harness settings.
It does not resume an ordinary native session or inherit its pass/repair state.

The owner JSON selects `{"model":"gpt-5.5","provider":{"apiKey":"OWNER_KEY"}}`
for the standard Responses endpoint. Optional `provider.baseUrl` selects an
owner endpoint; a local deterministic transport may omit the key. Unknown
configuration fields and other models fail incomplete. The pinned runtime's
other built-in model profiles override disabled tool settings with code mode;
their tool settlement is outside this qualified route. Do not assume model
names with similar capabilities have the same runtime contract.
Owner JSON must be valid UTF-8; malformed encoding is rejected before any runtime
command. Ambient system/managed Codex configuration is outside this route and
returns incomplete; the guard checks presence metadata without loading values.

The single JSON line on stdout has `status`, `diagnostics`, and `repairTurns`.
Only `complete` includes `modelText` and `candidate` (`identity`, `tree`,
`artifact`). Exit 0 requires complete; all other terminal results exit 3. The
caller owns the returned artifact until downstream acceptance or cleanup.
Native model text and hooks cannot set this result. SIGINT/SIGTERM cancel owned
subprocesses and return incomplete, with uncertain cleanup explicitly diagnosed.

Defaults are three repair turns and a 900-second total deadline including model
work, capture, checks, and cleanup. `--max-repair-turns N` and
`--deadline-seconds N` adjust those finite bounds. Infrastructure/consent errors
stop immediately. `--ignored-input path` repeats for required ignored files.
Concurrent attempts for one canonical workspace are rejected. A stale lock
never authorizes work; verify the previous attempt has stopped before removing
its attempt lock. The lock directory is
`<git-common-dir>/ironlint-completion-<sha256(canonical-root-path)>.lock`;
the canonical path is UTF-8 without a trailing newline. Verify all previous
controller/runtime/check processes have settled before manual removal.
Every new launch performs fresh acceptance.

The [Pi candidate contract](../pi/completion.md#candidate-and-evaluation) is reused
directly: tracked and nonignored untracked working bytes, dirty/partially staged
files, deletions and executable bits, plus declared ignored inputs. Capture does
not stage, commit, or change the branch. Unsupported file types/roots fail
incomplete. All exact required acceptance checks run against the captured tree.
Source drift triggers bounded recapture; a check that changes the evaluated
artifact invalidates success even if it exits zero. Abandoned artifacts are
removed after confirmed settlement. If cleanup cannot be confirmed within the
total deadline, the result is incomplete and the lock/artifact are retained
conservatively. A library caller still running can clean them after eventual
settlement; after standalone CLI exit the owner must verify process settlement
and remove retained attempt state. Never reuse that workspace while writes may
remain in flight.

This provides local workflow completion under the same-account assumptions in
Pi's contract. Manifest comparisons detect persistent drift; they cannot rule
out concurrent write-and-restore attacks or same-account tampering. Stronger v1
enforcement still requires independently controlled authority and isolation.
The exact macOS and Linux aarch64 qualification is recorded in the evidence file.
Linux replay passed with the outer Docker syscall filter unconfined and Codex's
own sandbox enabled, without privileged mode or added capabilities. The tested
default Docker filter denied patch writes and remains unsupported for qualified
repair; blocked sandbox operations return incomplete. Windows is outside this
packet. See the evidence record before claiming a tested platform or environment.
