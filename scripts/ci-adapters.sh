#!/usr/bin/env bash
set -euo pipefail

# W4 (specs/2026-08-17-git-floor-hook-and-self-defense-design.md): one lane
# for every adapter contract suite — the two Rust hook-contract suites plus
# the pi (node) and opencode (bun) suites, all consuming the pinned
# provenance-stamped fixtures in adapters/<harness>/fixtures/.
#
# Fails LOUDLY when tooling is missing (never silently skips a harness): a
# skipped suite is a harness that silently degrades.

cd "$(dirname "$0")/.."

# The pi/opencode suites spawn `ironlint` from PATH; build the binary so the
# lane is self-contained (CI already has a release artifact, but a local run
# must not depend on a global install). The binary lands in a dir whose NAME
# contains "ironlint": the pi suite's missing-binary test scrubs PATH entries
# matching /ironlint/, so a dir named `target/debug` would leak the fresh
# binary and break that test.
echo "ci-adapters: building ironlint…"
cargo build -p ironlint-cli
mkdir -p target/ironlint-bin
cp target/debug/ironlint target/ironlint-bin/ironlint
export PATH="$(pwd)/target/ironlint-bin:${PATH}"

echo "ci-adapters: Rust hook-contract suites…"
cargo test -p ironlint-cli --test hook_contract_claude_code --test hook_contract_codex

echo "ci-adapters: pi suite (node)…"
if ! command -v node >/dev/null 2>&1; then
  echo "ci-adapters: node is required for the pi adapter suite — install it" >&2
  exit 1
fi
(cd adapters/pi && npm test)

echo "ci-adapters: opencode suite (bun)…"
if ! command -v bun >/dev/null 2>&1; then
  echo "ci-adapters: bun is required for the opencode adapter suite — install it" >&2
  exit 1
fi
(cd adapters/opencode && bun test)

echo "ci-adapters: all adapter suites passed."
