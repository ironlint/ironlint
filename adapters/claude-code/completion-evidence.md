# Claude Code completion capability and qualification

Recorded 2026-10-02. Conclusion: **a controlled entry point is required**.
Ordinary native Stop hooks cannot own successful completion: persistent blocking,
hook failures, timeouts, and disabled registrations can all end with a successful
headless result. The supported completion result belongs to the explicit
`ironlint-claude-complete` controller, after fresh candidate acceptance.

## Runtime and primary evidence

The exact observed runtime was Claude Code **2.1.207**, macOS arm64, native
executable `/Users/chrisarter/.local/share/claude/versions/2.1.207`, SHA-256
`1397a062c6889675055e3314dd956376ac51262a7734ad9e819c26975d71547a`.
This is an exact support pin, not a qualified version range. Node 24.19.0
executed the macOS controller and the existing debug IronLint binary executed
schema-7 acceptance.

The same 24-case replay also passed on **Debian Bookworm Linux arm64 in a
standard `rust:1.88-bookworm` container**, using the exact Claude Code 2.1.207
native Linux runtime and official Node 24.21.0. Linux runtime SHA-256:
`8bc14a284065383460f37981d724b8f7aa7ca93c9849d2fe367e08f03383f454`.
The immutable `@anthropic-ai/claude-code-linux-arm64@2.1.207` registry metadata
supplied the verified tarball SRI:
`sha512-Jmcvn8Sg8+FaPLOy9t4h7ip+K1i5YYrimT1+iZL1YHBTA44WHJ9H/F8DW3QKnVyqUGqEza9Tg5PPVjzhi5fwyg==`.
Real Linux IronLint was built with `--locked` in the cached Rust image. The
repository mount was read-only; qualification files, projects and consent were
external temporary directories. No privileged container mode or added
capabilities were used. This qualifies that exact Linux container/platform
combination, not arbitrary Linux distributions or CPU architectures.

Context7 initially used the skill reference's exact IDs `/websites/code_claude`
and `/anthropics/claude-code`. The repository's resolve-first requirement was
then followed explicitly: `resolve-library-id` for Claude Code selected the
high-reputation official `/websites/code_claude` source (87.19 benchmark,
7,075 snippets), followed by a fresh query reconfirming the Stop contract.
Cross-checks used the
[official hook reference](https://code.claude.com/docs/en/hooks),
[hook guide](https://code.claude.com/docs/en/hooks-guide), and
[primary changelog, 2.1.207](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md#21207).
Current docs describe the continuation cap, nonblocking command-hook timeout,
and interruption bypass. They have newer-version details; the pinned executable's
actual dispatcher and real replay take precedence for this support pin.

The native executable contains the actual bundled dispatcher source. Searching
its bytes for `CLAUDE_CODE_STOP_HOOK_BLOCK_CAP`, `stop_hook_active`,
`preventContinuation`, and `error_max_turns` established the paths below:
`ije` constructs Stop payloads; `pXr` maps `continue:false` to
`preventContinuation`; the completion loop returns `reason:"completed"` when
the ninth consecutive Stop block exceeds its default cap; the headless result
uses `terminal_reason` independently of `subtype:"success"`. Runtime bytes are
identified by the executable hash rather than unstable minified symbol names.

## Real native outcomes

`adapters/shared/completion/test/claude-runtime.py` runs the real pinned CLI
against a localhost Anthropic SSE fixture with a dummy key. HOME, XDG, Claude
configuration, projects and IronLint consent are temporary. It never accesses
the user's live Claude hooks, authentication or IronLint trust store. No actual
model service is invoked; the provider deliberately ignores repair guidance.
The runtime's `CLAUDE_CODE_CERT_STORE=bundled` selector avoids macOS system-CA
initialization. Earlier probes that omitted it stalled before any provider
request, and were terminated by their process deadlines.

| Scenario | Actual pinned host outcome |
| --- | --- |
| Clean Stop | Exit 0, success, `terminal_reason: completed` |
| Persistent `decision:block` | Nine Stop callbacks: first false, then eight true; exit 0, success, `completed` |
| `continue:false` | Exit 0, success, `stop_hook_prevented` |
| Competing block and `continue:false` | Exit 0, success, `stop_hook_prevented` |
| Warning through `systemMessage` | Exit 0, success, `completed` |
| Hook process exits 1 | Exit 0, success, `completed` |
| Command hook timeout | Exit 0, success, `completed` |
| Disabled Stop registration | Zero Stop callbacks; exit 0, success, `completed` |
| Two-turn cap with persistent block | Exit 1, `error_max_turns`, `max_turns` |

`decision:block` requests another model turn; it cannot establish bounded
terminal incomplete work. `continue:false` terminates processing but does not
change the top-level success subtype. `systemMessage` is display feedback.
Neither route settles an immutable candidate or prevents independently queued,
detached, nested or externally initiated writes. Interactive sessions, resumed
sessions, plugin marketplace downloads and authenticated model repair utility
are not qualified by this replay. Both qualified platforms produced all native
outcomes in the table. During the Linux cancellation case, the localhost
fixture subsequently observed a broken pipe while writing to the deliberately
cancelled CLI; the owned result was incomplete and replay assertions passed.

## Controlled entry point

The adapter launches a fresh `--bare --print` session for each model turn, with
no persistence/resume, no automatic candidate CLAUDE.md discovery, no candidate
settings sources, no slash commands, no Chrome, and an empty strict MCP config.
The only exposed tools in the pinned bare runtime are **Read and Edit**. Edit
with an empty old string creates a new file; this was exercised against the real
runtime. Bash, Agent, scheduling, detached processes and alternate repair flags
are unavailable. Permissions use `dontAsk`, canonical candidate paths, scoped
allow rules and Git metadata deny rules. Root paths containing permission-rule
metacharacters or control characters are rejected. Paths containing spaces were
exercised successfully. Provider configuration is an external owner file,
limited to 64 KiB with strict UTF-8 decoding and an allowlist of API key/base URL,
model and a bounded turn limit.

The real provider replay proved candidate file creation, denied outside Read and
Edit, denied `.git/config` creation, and ignored candidate-local hook settings
and auto-discovered CLAUDE.md instructions. These are local host permission
checks, not an OS sandbox or independent publication authority. The shared
controller protects owner paths, owns process settlement, captures the existing
Pi candidate contract, checks the complete owner-selected ID set, rejects altered
evaluated artifacts, recaptures late source changes within bounds, and emits
complete only after a fresh complete acceptance pass.

With real IronLint and isolated consent, the replay established:

| Controlled scenario | Owned result |
| --- | --- |
| Clean acceptance | Complete, zero repair turns |
| Initial violation repaired by actual Edit | Complete, one repair turn |
| Persistent violation, ignored repair guidance | Incomplete, one configured repair turn exhausted |
| Missing local consent | Incomplete, zero repair turns |
| Caller cancellation during provider work | Incomplete, zero repair turns; owned CLI stopped |
| Check mutates captured source and exits zero | Incomplete; evaluated candidate changed |
| Check repeatedly changes original source during finalization | Incomplete after bounded recaptures |

The fixture verifies original Git index bytes remain unchanged. It explicitly
disposes successful candidate artifacts as their caller owner and verifies no
abandoned artifacts remain. Same-account external writes and write-and-restore
attacks remain outside the independent six-part enforcement contract. The
claimed result names the captured candidate artifact, not all mutable workspace
bytes after it returns.

Two of the 24 cases exercise the actual `bin/ironlint-claude-complete` shell
launcher and CLI on both qualified platforms, with explicit absolute owner
flags, an isolated Node PATH, real IronLint and temporary consent. A clean
candidate produces exactly one JSON result with status `complete`, exit 0,
held model text and the exact candidate `identity`/`tree`/`artifact` projection.
A persistent violation with zero authorized repair turns produces one
`incomplete` JSON result, exit 3, and no candidate or model text. Both cases
preserve the original index and source bytes. The caller disposes the completed
artifact, and no abandoned artifact remains.

Reproduce with absolute owner-selected paths:

```sh
rtk proxy python3 adapters/shared/completion/test/claude-runtime.py \
  --runtime /absolute/path/to/claude-2.1.207 \
  --node /absolute/path/to/node \
  --ironlint /absolute/path/to/ironlint
```

## Adapter drift audit — claude-code (2026-10-02)

Baseline: 2026-05-28 against 2.1.89, initial unverified baseline.
Current observed: 2.1.207, with current primary documentation cross-checks.

### Drift (⚠️)

- [contract #5: completion authority] `adapters/shared/hooks/hook.py:127`
  now: actual native Stop blocking is capped and hook failure/timeout can finish
  successfully. Source: official hook reference and pinned real-runtime replay.
  was: the legacy response mapping requests one repair, then supplies a warning.
  recommend: keep ordinary hooks as feedback and use the explicit owned
  completion controller. Returning block forever or `continue:false` does not
  repair the native successful-result gap.
- The reference's old `hook.sh` parser, SessionStart clearing, LLM evaluator,
  and evaluator-subagent consumers no longer exist in the current adapter.
  Its surface map therefore overstates the current consumers of #1/#4/#6/#9/#10;
  review the reference separately before adopting its old line anchors.

### New capabilities not adopted (✨)

- `terminal_reason` distinguishes a prevented Stop from a completed headless
  turn, although both can have success subtype. The controlled path adopts this
  distinction and rejects every terminal reason except `completed`.

### Unverifiable (❓)

- [contract #7] Marketplace plugin installation against a supported archive
  runtime is outside the installed 2.1.207 support pin; package tests alone do
  not qualify that distribution route.
- [contracts #8/#9/#10] Skill/subagent model behavior is not a completion
  capability consumed by this explicit restricted controller. No live-model or
  per-dispatch model-override claim was verified.

### In sync (✅)

- Current #1 PostToolUse/Stop names, #2 post-edit matcher scope, #3 plugin-root
  command interpolation, #4 basic cwd/event/tool input shape, and #6 structured
  PostToolUse feedback match primary hooks documentation. Completion acceptance
  still runs all required checks independently of post-edit delivery.
- #5 ordinary Stop response syntax is accepted by the real pinned dispatcher;
  its limited completion authority is recorded above.

### Proposed watermark

Last verified: 2026-10-02 against Claude Code 2.1.207 (changelog entry: 2.1.207;
native dispatcher replay; ordinary hooks remain feedback-only). This is a
proposal only; the skill reference watermark was not edited.
