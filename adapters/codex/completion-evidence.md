# Codex completion capability and qualification

Recorded 2026-10-02 on macOS (`Darwin 25.5.0 arm64`). Runtime:
`codex-cli 0.159.1`, installed executable
`/Users/chrisarter/.nvm/versions/node/v24.19.0/bin/codex`.
The pinned primary source is tag `rust-v0.159.1`, revision
`8e68a98ef03cdde76d2e6800791ebdf1b3b95b24` in
[openai/codex](https://github.com/openai/codex/tree/8e68a98ef03cdde76d2e6800791ebdf1b3b95b24).
This observation qualifies an exact runtime and route, not a version range.

## Capability decision

**A controlled entry point is needed.** Native Stop hooks cannot record bounded
terminal incomplete work or bind successful completion to a settled candidate.
The ordinary plugin remains post-edit feedback and first-Stop repair guidance.
Its warning on continued failure is not an enforced completion result.

The actual installed runtime was replayed through a local HTTP Responses
provider. The model transport supplies deterministic responses; the unmodified
runtime performs hook dispatch, tool execution, turn finalization, cancellation,
and JSONL serialization. These tests do not simulate hook callbacks or replace
the runtime. No live model credential, developer home/config, trust store, or
installed developer hooks were used. Broad model repair utility remains separate.

| Probe through the real runtime | Observed host result |
| --- | --- |
| First JSON `decision:block` with reason | Another model request; subsequent Stop has `stop_hook_active:true` |
| Exit 2 with stderr reason | Same continuation behavior |
| Continued warning-only response | Exit 0 and `turn.completed` despite warning |
| Repeated block responses | Further model requests; repeated payloads remain `stop_hook_active:true`; no built-in repair limit |
| `continue:false` / exhausted reason | Normal exit 0 and `turn.completed`, not terminal incomplete |
| Hook exit 1 or one-second hook timeout | Normal exit 0 and `turn.completed` |
| Missing or disabled Stop hooks | Normal exit 0 and `turn.completed` without callback |
| Competing block and `continue:false` hooks | `continue:false` wins; normal exit 0 and `turn.completed` |

`systemMessage` is a warning. Stop `decision:block` is a continuation prompt.
Neither denotes a host-owned incomplete terminal result. Source confirms the
observations: [`events/stop.rs`](https://github.com/openai/codex/blob/8e68a98ef03cdde76d2e6800791ebdf1b3b95b24/codex-rs/hooks/src/events/stop.rs#L257)
classifies outputs and gives stopped hooks precedence; [`session/turn.rs`](https://github.com/openai/codex/blob/8e68a98ef03cdde76d2e6800791ebdf1b3b95b24/codex-rs/core/src/session/turn.rs#L649)
feeds block reasons to the model and breaks normally on `should_stop`.
The [official hooks reference](https://learn.chatgpt.com/docs/hooks) describes
the same control effects and warns that specialized tool paths can bypass hooks.

Native Stop evaluates mutable workspace bytes. Its payload has no immutable
candidate artifact and no assurance that background or queued writers have
settled. The hook dispatcher does not confer independent publication authority.

## Supported controlled route

`bin/ironlint-codex-complete` explicitly launches the packaged adapter controller.
It owns the terminal JSON `status:complete|incomplete` and exit status. Raw
`codex exec` completion is an intermediate model result. Success additionally
requires fresh complete schema-7 acceptance of the captured candidate, exact
expected IDs, owner-input stability, and source/artifact identity checks.

The qualified model configuration is **`gpt-5.5` only**. The pinned built-in
catalog sets other exposed models to `code_mode_only`; that model metadata
overrides feature toggles. Disabling the code-mode host leaves those models
without a repair tool route. Unsupported model settings fail before model work.
[`tools/mod.rs`](https://github.com/openai/codex/blob/8e68a98ef03cdde76d2e6800791ebdf1b3b95b24/codex-rs/core/src/tools/mod.rs#L75)
and the [pinned catalog](https://github.com/openai/codex/blob/8e68a98ef03cdde76d2e6800791ebdf1b3b95b24/codex-rs/models-manager/models.json)
establish this restriction.

Owner JSON has exactly:

```json
{"model":"gpt-5.5","provider":{"apiKey":"explicit-owner-key","baseUrl":"https://api.openai.com/v1"}}
```

`baseUrl` defaults to the official HTTPS API; an explicit HTTP loopback endpoint
is also allowed for replay. An explicit API key is required for the remote API.
No arbitrary configuration flags, inherited credential environment, stored
authentication, or developer configuration are accepted. Runtime and owner JSON
paths are absolute and outside the candidate; the controller fingerprints them.

Codex also loads ambient system and macOS MDM sources despite isolated HOME and
`--ignore-user-config`. This route rejects any present
`/etc/codex/config.toml`, `/etc/codex/requirements.toml`, or
`/etc/codex/managed_config.toml`. On macOS it uses the same CoreFoundation
`CFPreferencesAppValueIsForced` predicate as the pinned runtime for
`com.openai.codex` keys `config_toml_base64` and
`requirements_toml_base64`, requiring confirmed absence. Only presence metadata
is checked; no preference value is loaded or printed. A failed probe is
incomplete. This guard prevents an ambient managed MCP or hook configuration
from starting external work outside the selected owner launch contract.
System path checks use `lstat`: a dangling symlink is present, and only ENOENT
or ENOTDIR confirms absence. Permission and other metadata errors are incomplete.
The pinned [configuration loader](https://github.com/openai/codex/blob/8e68a98ef03cdde76d2e6800791ebdf1b3b95b24/codex-rs/config/src/loader/mod.rs#L112)
and [macOS loader](https://github.com/openai/codex/blob/8e68a98ef03cdde76d2e6800791ebdf1b3b95b24/codex-rs/config/src/loader/macos.rs#L150)
identify those sources and predicates.

Each model turn is a fresh ephemeral invocation with separate HOME, XDG and
CODEX_HOME directories. The launcher explicitly marks the candidate untrusted,
ignores user config and execpolicy rules, disables hooks/plugins/MCP/apps,
shell tools, nested agents, and code mode, uses `workspace-write` with temporary
writable roots excluded, and supplies the prompt on stdin. The real provider
request exposes synchronous `apply_patch`, with no shell, exec, MCP, or agent
execution tool. Candidate-owned `.codex/config.toml` and hooks cannot re-enable
these routes. External owner patch attempts leave owner bytes unchanged.

The JSONL consumer requires exit 0, one fresh thread/turn sequence, one explicit
`turn.completed`, settled supported items, valid UTF-8, and no error or trailing
events. Command, MCP, collaboration, unknown, failed patch, or pending item
events are incomplete. A request for a nonexistent disabled tool is rejected by
Codex and can be absent from JSONL; it executes no tool and grants no acceptance.
Fresh acceptance still determines the controller's terminal result.
[`event_processor_with_jsonl_output.rs`](https://github.com/openai/codex/blob/8e68a98ef03cdde76d2e6800791ebdf1b3b95b24/codex-rs/exec/src/event_processor_with_jsonl_output.rs#L513)
maps Completed to `turn.completed`, Failed to `turn.failed`, and Interrupted to
shutdown without a completion event. The controller never treats exit 0 alone
as success.

## Verification

`adapters/shared/completion/test/codex-runtime.test.ts` exercises the installed
runtime against a local fixture, including actual patch repair and a real
IronLint binary. Recorded passing cases:

- Clean candidate completes and returns its captured artifact.
- A violation repairs through actual Codex `apply_patch`, then fresh acceptance
  passes with one repair.
- A model that repeatedly says done exhausts one repair and records incomplete.
- Provider error and caller cancellation record incomplete with no candidate.
- A check that changes its evaluated candidate is rejected.
- A check that repeatedly changes source during finalization exhausts bounded
  recapture and records incomplete.
- Missing shell tools cannot write; owner-file patch attempts cannot change
  owner configuration; candidate hooks/config do not enable excluded routes.
- System configuration and MDM guards reject present or uncertain sources
  before starting model work; tests inject metadata queries without changing
  any real system configuration.
- Native callback payloads and all host outcomes in the capability table are
  observed through the exact installed runtime.
- The actual `bin/ironlint-codex-complete` shell launcher is also exercised with
  the real runtime and evaluator: clean work emits exactly one terminal JSON
  line and exit 0 with only the documented candidate projection/model text;
  a zero-repair exhausted violation emits exactly one incomplete result, exit
  3, and no candidate or model text. The test caller removes the transferred
  successful artifact.
- Invalid owner JSON UTF-8 is rejected before any runtime command; a regression
  with an invalid API-key byte and a runtime-start marker fails on replacement
  decoding and passes with fatal decoding.

The replay command (with RTK as required by this repository) is:

```sh
rtk proxy env \
  IRONLINT_CODEX_RUNTIME=/Users/chrisarter/.nvm/versions/node/v24.19.0/bin/codex \
  IRONLINT_BIN=/Users/chrisarter/Documents/projects/ironlint/target/debug/ironlint \
  node --experimental-strip-types --test \
  adapters/shared/completion/test/codex.test.ts \
  adapters/shared/completion/test/codex-runtime.test.ts
```

The optional real-runtime tests skip when `IRONLINT_CODEX_RUNTIME` is absent;
that skip is not qualification evidence. Controller/evaluator cases additionally
require `IRONLINT_BIN`. All consent is granted solely to temporary probe policies
inside temporary XDG stores. The normal package/adapter lane verifies the same
shared schema and Pi candidate contract independently.

### Linux qualification

The full six real-runtime tests also passed on Linux aarch64 in the local
`rust:1.88-bookworm` Docker environment with `--security-opt seccomp=unconfined`.
The repository mount was read-only, generated/downloaded artifacts lived in a
separate temporary qualification mount, and `TMPDIR` was
`/qualification/replay-tmp`. No privileged mode or additional capabilities were
used. The Codex `workspace-write` sandbox and owner-directory exclusions remained
enabled; the outer Docker syscall filter was the only adjusted container setting.

Exact qualification inputs:

| Input | Pin |
| --- | --- |
| Codex Linux aarch64 MUSL executable | 0.159.1; SHA-256 `22c787768933ff4d97e62e2d4613e1671e18b6a2cc999f0666be544acecffa45` |
| Codex platform package integrity | `sha512-SUuxUCTWkvmzrCPIpRHMlivwSsL2STeUv6Dqm/TkUwYFfaqAQqZ8uIq8+d3SnYJsXiz1bkoiIdy8CSuptfEBfQ==` |
| Official Node Linux arm64 runtime | 24.21.0; SHA-256 `6ad1325edbdb5649c379b75a237147a666c95d4f9ae8d340fef2d1575d289ad2` |
| Rust Docker image | `rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0` |
| Observed local image ID | `sha256:9ad0c47880881464036f8b5779f49a15d058154581404d0d50a20f9d11f3436e` |

Reproduce after preparing the verified runtime packages and building the locked
IronLint evaluator in an external qualification directory:

```sh
rtk proxy docker run --rm --security-opt seccomp=unconfined \
  --mount "type=bind,source=$REPO,target=/workspace,readonly" \
  --mount "type=bind,source=$QUALIFICATION,target=/qualification" \
  --workdir /workspace \
  --env TMPDIR=/qualification/replay-tmp \
  --env IRONLINT_CODEX_RUNTIME=/qualification/codex/package/vendor/aarch64-unknown-linux-musl/bin/codex \
  --env IRONLINT_BIN=/qualification/target/debug/ironlint \
  rust@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 \
  /qualification/node/node-v24.21.0-linux-arm64/bin/node \
  --experimental-strip-types --test --test-concurrency=1 \
  adapters/shared/completion/test/codex-runtime.test.ts
```

`REPO` is the source checkout and `QUALIFICATION` is the caller-owned temporary
directory containing the pinned packages and evaluator. The recorded run used
`/Users/chrisarter/Documents/projects/ironlint` and
`/private/tmp/ironlint-linux-native-jox1lsiz`, respectively; the temporary directory
is removed after qualification.

The same suite with Docker's default syscall filter passed four cases but failed
the two cases that require patch writes. Codex reported failed writes and the
controller withheld success. Moving fixture roots outside `/tmp` did not resolve
that failure. That default-filter environment is **unsupported for qualified
repair**, and no broader Linux environment or sandbox version claim follows from
this run. Linux support requires the tested sandbox operations to be available;
blocked operations remain incomplete.

macOS and the specific Linux environment above are qualified.
Ordinary interactive/native routes, resume, arbitrary model/tool configurations,
and other runtime versions have no controlled completion claim. Same-account
checks remain trusted code; snapshots and before/after comparisons do not defeat
all write-and-restore attacks or provide independent publication authority.

## Adapter drift audit — codex (2026-10-02)

Baseline: no Codex reference watermark exists in the repository audit skill.
Current: installed Codex 0.159.1; primary source revision above.

### Unverifiable (❓)

- The audit skill references `references/codex.md`, but only Claude Code and Pi
  references exist. A prior watermark and complete historical contract surface
  cannot be reconstructed from that absent file.
- No qualified version range or live remote model utility measurement is claimed.

### In sync (✅)

- Current PostToolUse and Stop command registration, 610-second native timeout,
  `PLUGIN_ROOT`, and apply_patch `Edit|Write` aliases match the pinned runtime and
  official hooks documentation.
- Fresh schema/check-set validation matches the existing evaluator contract;
  native feedback still intentionally does not claim terminal enforcement.
- Current primary docs were obtained by resolving OpenAI Codex through Context7,
  selecting `/openai/codex`, and querying Stop, errors, cancellation and exec
  completion semantics, then checking official docs and exact pinned source.

### Proposed watermark

Last verified: 2026-10-02 against Codex 0.159.1 (primary source:
`rust-v0.159.1`, `8e68a98ef03cdde76d2e6800791ebdf1b3b95b24`).
The existing skill/reference files were not modified.
