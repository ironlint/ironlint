# Pi adapter contract reference

## Thesis

The project extension reports post-edit feedback. The separate controlled runner
owns task completion and must wait for Pi to settle before final acceptance.

## Watermark

Last verified: 2026-10-01 against Pi 0.87.1 (source f07218c4d4bbc12bef056a7058c3dd49dfe41abe).

## Doc sources

- [Pi 0.87.1 SDK](https://github.com/earendil-works/pi/blob/f07218c4d4bbc12bef056a7058c3dd49dfe41abe/packages/coding-agent/docs/sdk.md)
- [Pi 0.87.1 extensions](https://github.com/earendil-works/pi/blob/f07218c4d4bbc12bef056a7058c3dd49dfe41abe/packages/coding-agent/docs/extensions.md)
- [Pi releases](https://github.com/earendil-works/pi/releases)

## Contract surface map

| # | Contract | Adapter surface |
| --- | --- | --- |
| 1 | `tool_result` follows completed tool execution; handlers may add content | `adapters/pi/src/index.ts` |
| 2 | `agent_end` can precede retry or queued work | `adapters/pi/src/completion.ts` |
| 3 | `agent_settled` means no automatic work remains | `adapters/pi/src/completion.ts` |
| 4 | SDK host owns `prompt`, `abort`, `waitForIdle`, and `dispose` | `adapters/pi/src/completion.ts` |
| 5 | Extension resource loading and project trust | `adapters/pi/src/sdk.ts` |
