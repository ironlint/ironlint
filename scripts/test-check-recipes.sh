#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${IRONLINT_TEST_BIN:-$repo/target/debug/ironlint}"
test -x "$binary" || { echo "build ironlint or set IRONLINT_TEST_BIN" >&2; exit 1; }
temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
export XDG_CONFIG_HOME="$temporary/config"
mkdir -p "$XDG_CONFIG_HOME"

for recipe in architecture dependency generated; do
  for scenario in pass fail repaired; do
    root="$temporary/$recipe-$scenario"
    mkdir -p "$root/.ironlint"
    cp -R "$repo/tests/fixtures/check-recipes/$recipe/$scenario/." "$root/"
    cp -R "$repo/examples/checks/$recipe/.ironlint/." "$root/.ironlint/"
    cp -R "$repo/examples/checks/$recipe/docs" "$root/docs"
    cp "$repo/examples/checks/$recipe/.ironlint.yml" "$root/.ironlint.yml"
    git -C "$root" init -q
    "$binary" trust --config "$root/.ironlint.yml" >/dev/null
    status=0
    "$binary" check --config "$root/.ironlint.yml" --root "$root" --event accept --format json >"$temporary/verdict.json" || status=$?
    python3 - "$recipe" "$scenario" "$status" "$temporary/verdict.json" <<'PY'
import json
from pathlib import Path
import sys

recipe, scenario, exit_status, path = sys.argv[1:]
verdict = json.loads(Path(path).read_text())
expected = "pass" if scenario != "fail" else "violation"
assert verdict["schema"] == 7, verdict
assert verdict["event"] == "accept", verdict
assert verdict["status"] == expected, verdict
assert [result["id"] for result in verdict["results"]] == [recipe], verdict
assert not verdict["not_run"] and verdict["error"] is None, verdict
assert int(exit_status) == (0 if scenario != "fail" else 2), verdict
if scenario == "fail":
    result = verdict["results"][0]
    assert result["outcome"] == "violation", verdict
    output = bytes(result.get("stdout", []) + result.get("stderr", [])).decode("utf-8")
    assert "see docs/" in output and ":" in output, verdict
print(f"{recipe} {scenario}: {expected}")
PY
  done
done
