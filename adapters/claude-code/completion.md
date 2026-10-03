# Explicit Claude Code completion

`bin/ironlint-claude-complete` owns a local task result separately from ordinary
Claude Code sessions and Stop hooks. Use the extracted adapter package or this
source checkout with Node.js 22.6+ on Linux/macOS. No extra hook is installed.
Exact runtime support and observed replay results are recorded in
[completion evidence](completion-evidence.md); unavailable qualification remains
pending and other versions return incomplete.

Claude Code 2.1.207 is qualified on macOS arm64 with Node 24.19.0, and on
Debian Bookworm Linux arm64 in the standard `rust:1.88-bookworm` container with
Node 24.21.0. The evidence pins each runtime by hash and records real dispatcher
and integrated controller replay, including the actual launcher JSON/exit 0/3
channel. Other Linux distributions, architectures and
authenticated model repair utility remain outside that qualification.

```sh
./adapters/claude-code/bin/ironlint-claude-complete \
  --root /absolute/candidate-repo \
  --policy /absolute/owner/policy.yml \
  --binary /absolute/ironlint \
  --artifacts /absolute/owner/artifacts \
  --runtime /absolute/claude \
  --runtime-config /absolute/owner/claude.json \
  --checks fmt,test \
  --task 'Implement the requested change'
```

The external owner configuration is JSON with a nonempty `model`, optional
`maxTurns` (1–16), and an `env` object restricted to supported Anthropic provider
settings. Provider keys are supplied explicitly by the owner; live Claude
configuration is not inherited. For a local deterministic transport:

```json
{"model":"claude-sonnet-4-5","maxTurns":4,"env":{"ANTHROPIC_API_KEY":"local-fixture","ANTHROPIC_BASE_URL":"http://127.0.0.1:8080"}}
```

The controller disables native hook/plugin/MCP/background entry points and
restricts model tools according to the pinned runtime contract. Policy,
evaluator, runtime/configuration, controller, and artifact store must stay
owner-controlled outside the candidate root. Review and trust the policy
separately; completion never grants consent or executes diagnostic repair
commands. It does not resume or replace an ordinary native session.

The [shared completion contract](../codex/completion.md) specifies the single
`complete`/`incomplete` JSON terminal result, exit 0/3, caller-owned candidate
artifact, three-repair/900-second defaults, cancellation, workspace serialization,
and fresh full acceptance. Optional `--ignored-input path`,
`--max-repair-turns N`, and `--deadline-seconds N` behave identically.
Only a strict settled host result followed by a complete acceptance pass can
release held model text. Model “done,” hook warnings, errors, exhausted turns,
and uncertain settlement never authorize success.

The [existing Pi candidate contract](../pi/completion.md#candidate-and-evaluation)
is reused directly without a second definition. This is local workflow
completion under same-account assumptions, with no independent publication
authority or protection against concurrent write-and-restore attacks.
Real-runtime qualification is separate from simulated callbacks and model repair
utility measurement. Consult the evidence record for the tested runtime/platform.
