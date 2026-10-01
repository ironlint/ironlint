#!/usr/bin/env bash
set -euo pipefail

# Pi is the supported v1 adapter. Its suite exercises completed-edit feedback.

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

echo "ci-adapters: pi suite (node)…"
if ! command -v node >/dev/null 2>&1; then
  echo "ci-adapters: node is required for the pi adapter suite — install it" >&2
  exit 1
fi
(cd adapters/pi && npm test)
bash scripts/test-verify-acceptance.sh
bash scripts/test-check-recipes.sh

echo "ci-adapters: all adapter suites passed."
