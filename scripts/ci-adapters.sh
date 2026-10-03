#!/usr/bin/env bash
set -euo pipefail

# Harness qualification is separate from the core release gate.

cd "$(dirname "$0")/.."

# The Pi suite spawns `ironlint` from PATH; build the binary so the
# lane is self-contained (CI already has a release artifact, but a local run
# must not depend on a global install). The binary lands in a dir whose NAME
# contains "ironlint": the pi suite's missing-binary test scrubs PATH entries
# matching /ironlint/, so a dir named `target/debug` would leak the fresh
# binary and break that test.
echo "ci-adapters: building ironlint…"
cargo build --locked -p ironlint-cli
mkdir -p target/ironlint-bin
cp target/debug/ironlint target/ironlint-bin/ironlint
export PATH="$(pwd)/target/ironlint-bin:${PATH}"
export IRONLINT_TEST_BIN="$(pwd)/target/ironlint-bin/ironlint"
export PYTHONDONTWRITEBYTECODE=1

echo "ci-adapters: native hook contracts and independent packages (python3)…"
python3 -m unittest discover -s adapters/shared/test -v

if ! command -v node >/dev/null 2>&1; then
  echo "ci-adapters: node is required for the pi adapter suite — install it" >&2
  exit 1
fi
echo "ci-adapters: controlled native completion (node)…"
node --experimental-strip-types --test --test-concurrency=1 adapters/shared/completion/test/*.test.ts
if [ -n "${IRONLINT_CLAUDE_RUNTIME:-}" ]; then
  python3 adapters/shared/completion/test/claude-runtime.py \
    --runtime "$IRONLINT_CLAUDE_RUNTIME" --node "$(command -v node)" --ironlint "$IRONLINT_TEST_BIN"
fi
echo "ci-adapters: pi suite (node)…"
(cd adapters/pi && npm test)
bash scripts/test-verify-acceptance.sh
bash scripts/test-check-recipes.sh

echo "ci-adapters: all adapter suites passed."
