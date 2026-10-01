#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat >"$tmp/ironlint" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$@" >"$IRONLINT_TEST_ARGS"
printf '%s\n' "$IRONLINT_TEST_VERDICT"
exit "${IRONLINT_TEST_EXIT:-0}"
EOF
chmod +x "$tmp/ironlint"

run() {
  IRONLINT_BIN="$tmp/ironlint" \
  IRONLINT_POLICY="$tmp/policy.yml" \
  IRONLINT_ROOT="$tmp/root" \
  IRONLINT_ACCEPTANCE_CHECKS="${IRONLINT_TEST_CHECKS:-fmt,clippy,test}" \
  IRONLINT_TEST_ARGS="$tmp/args" \
  "$root/scripts/verify-acceptance.sh" >/dev/null 2>&1
}

pass='{"schema":7,"event":"accept","status":"pass","results":[{"id":"clippy","outcome":"pass","exit_status":0,"stdout":[],"stderr":[],"stdout_truncated":false,"stderr_truncated":false,"reason":null},{"id":"fmt","outcome":"pass","exit_status":0,"stdout":[],"stderr":[],"stdout_truncated":false,"stderr_truncated":false,"reason":null},{"id":"test","outcome":"pass","exit_status":0,"stdout":[],"stderr":[],"stdout_truncated":false,"stderr_truncated":false,"reason":null}],"not_run":[],"error":null}'
mkdir "$tmp/root"
touch "$tmp/policy.yml"

IRONLINT_TEST_VERDICT="$pass" run
[[ "$(<"$tmp/args")" == "$(printf 'check\n--config\n%s\n--root\n%s\n--event\naccept\n--format\njson' "$tmp/policy.yml" "$tmp/root")" ]]

for verdict in \
  '{"schema":6,"event":"accept","status":"pass","results":[{"id":"clippy","outcome":"pass"},{"id":"fmt","outcome":"pass"},{"id":"test","outcome":"pass"}],"not_run":[],"error":null}' \
  '{"schema":7,"event":"change","status":"pass","results":[{"id":"clippy","outcome":"pass"},{"id":"fmt","outcome":"pass"},{"id":"test","outcome":"pass"}],"not_run":[],"error":null}' \
  '{"schema":7,"event":"accept","status":"pass","results":[{"id":"clippy","outcome":"pass"},{"id":"fmt","outcome":"pass"}],"not_run":[],"error":null}' \
  '{"schema":7,"event":"accept","status":"pass","results":[{"id":"clippy","outcome":"pass"},{"id":"fmt","outcome":"pass"},{"id":"test","outcome":"pass"}],"not_run":[{"id":"later","reason":"timeout"}],"error":null}' \
  '{"schema":7,"event":"accept","status":"pass","results":[{"id":"clippy","outcome":"error"},{"id":"fmt","outcome":"pass"},{"id":"test","outcome":"pass"}],"not_run":[],"error":null}'; do
  if IRONLINT_TEST_VERDICT="$verdict" run; then
    echo "accepted invalid verdict: $verdict" >&2
    exit 1
  fi
done

while IFS= read -r case; do
  name="$(jq -r .name <<<"$case")"
  kind="$(jq -r .kind <<<"$case")"
  verdict="$(jq -c .verdict <<<"$case")"
  exit_code="$(jq -r .exit <<<"$case")"
  if IRONLINT_TEST_VERDICT="$verdict" IRONLINT_TEST_EXIT="$exit_code" IRONLINT_TEST_CHECKS='fmt,test' run; then
    if [[ "$kind" != pass ]]; then echo "accepted $name" >&2; exit 1; fi
  elif [[ "$kind" == pass ]]; then
    echo "rejected $name" >&2
    exit 1
  fi
done < <(jq -c '.cases[]' "$root/tests/fixtures/acceptance-conformance.json")

if IRONLINT_TEST_VERDICT="$pass" IRONLINT_TEST_EXIT=3 run; then
  echo "accepted nonzero evaluator exit" >&2
  exit 1
fi

for verdict in "$pass"$'\n'"$pass" $'false\n'"$pass"; do
  if IRONLINT_TEST_VERDICT="$verdict" run; then
    echo 'accepted multiple JSON verdicts' >&2
    exit 1
  fi
done
