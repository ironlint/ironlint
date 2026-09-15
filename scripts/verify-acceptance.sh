#!/usr/bin/env bash
set -euo pipefail

: "${IRONLINT_POLICY:?set IRONLINT_POLICY to the trusted v1 policy}"
: "${IRONLINT_ROOT:?set IRONLINT_ROOT to the candidate root}"
: "${IRONLINT_ACCEPTANCE_CHECKS:?set IRONLINT_ACCEPTANCE_CHECKS to the required IDs}"

ironlint_bin="${IRONLINT_BIN:-ironlint}"
verdict="$(mktemp)"
trap 'rm -f "$verdict"' EXIT

if ! "$ironlint_bin" check --config "$IRONLINT_POLICY" --root "$IRONLINT_ROOT" --event accept --format json >"$verdict"; then
  cat "$verdict" >&2
  exit 1
fi

expected="$(jq -cn --arg ids "$IRONLINT_ACCEPTANCE_CHECKS" '$ids | split(",")')"
jq -se --argjson expected "$expected" '
  length == 1 and (.[0] |
  ($expected | length > 0)
  and ($expected | all(type == "string" and length > 0))
  and (($expected | length) == ($expected | unique | length))
  and (.schema == 7)
  and (.event == "accept")
  and (.status == "pass")
  and (.error == null)
  and (.not_run == [])
  and (.results | type == "array")
  and ([.results[].id] as $actual
       | (($actual | length) == ($actual | unique | length))
       and (($actual | sort) == ($expected | sort)))
  and (.results | all(.outcome == "pass"))
  )
' "$verdict" >/dev/null || {
  cat "$verdict" >&2
  exit 1
}
