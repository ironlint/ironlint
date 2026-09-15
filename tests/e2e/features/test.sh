#!/usr/bin/env bash
set -Eeuo pipefail
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
trap 'echo "FAIL at line $LINENO" >&2; cat "$tmp/stdout" "$tmp/stderr" >&2' ERR
export IRONLINT_PROJECT="$tmp/project"
export IRONLINT_POLICY="$tmp/policy.yml"
export IRONLINT_ACCEPTANCE_CHECKS='content,syntax'
mkdir -p "$IRONLINT_PROJECT/src"
cd "$IRONLINT_PROJECT"
git init -q
git config user.name 'IronLint fixture'
git config user.email 'fixture@example.test'
git config commit.gpgsign false

expect() {
  local expected="$1" actual=0
  shift
  "$@" >"$tmp/stdout" 2>"$tmp/stderr" || actual=$?
  if [[ "$actual" != "$expected" ]]; then
    echo "expected exit $expected, got $actual: $*" >&2
    cat "$tmp/stdout" "$tmp/stderr" >&2
    exit 1
  fi
}
verdict() { jq -e "$1" "$tmp/stdout" >/dev/null; }
commit() { git add -A; git commit -qm "$1"; }
accepts() {
  expect 0 ironlint-fixture-harness accept
  [[ "$(cat "$tmp/stdout")" == "$(git rev-parse HEAD)" ]]
}
denies() {
  expect 1 ironlint-fixture-harness accept
  [[ ! -s "$tmp/stdout" ]]
}

cat >"$IRONLINT_POLICY" <<'YAML'
version: 1
checks:
  content:
    run: "! grep -n FORBIDDEN src/*.txt"
    files: '*.txt'
    on: [change, accept]
  syntax:
    run: sh -n app.sh
YAML
printf 'allowed\n' >src/a.txt
printf 'allowed\n' >src/b.txt
printf 'echo ready\n' >app.sh
commit initial

expect 4 ironlint-fixture-harness change --file src/a.txt
verdict '.schema == 7 and .status == "error"'
ironlint trust --config "$IRONLINT_POLICY" >/dev/null
accepts
echo 'ok: installed CLI, isolated consent, complete acceptance'

printf 'FORBIDDEN\n' >src/a.txt
expect 2 ironlint-fixture-harness change --file src/a.txt
verdict '.event == "change" and .status == "violation" and .results[0].id == "content" and (.results[0].stdout | implode | contains("FORBIDDEN"))'
[[ "$(cat src/a.txt)" == FORBIDDEN ]]
commit red
denies
printf 'repaired\n' >src/a.txt
expect 0 ironlint-fixture-harness change --file src/a.txt
verdict '.status == "pass"'
# A repaired working tree cannot authorize the still-failing committed snapshot.
denies
git add src/a.txt
denies
git commit -qm repaired
accepts
echo 'ok: red remains editable; repair only accepts after the candidate is updated'

expect 0 ironlint-fixture-harness change --file src/a.txt --file src/b.txt --file src/a.txt
verdict '[.results[].id] == ["content"]'
expect 0 ironlint-fixture-harness change --file notes.md
verdict '.status == "not_run" and .results == []'
expect 0 ironlint-fixture-harness change
verdict '[.results[].id] == ["content"]'
echo 'ok: batch selection runs once; known nonmatch skips; unknown paths run'

# This write deliberately bypasses the fixture's feedback call.
printf 'if then\n' >app.sh
expect 0 ironlint-fixture-harness change --file src/a.txt
commit bad-syntax
denies
grep -q 'syntax' "$tmp/stderr"
printf 'echo repaired\n' >app.sh
commit fixed-syntax
accepts
echo 'ok: full acceptance catches a write omitted from early feedback'

printf 'FORBIDDEN\n' >src/a.txt
commit later-red
denies
# Candidate policy changes cannot replace the policy selected by this caller.
printf 'version: 1\nchecks:\n  fake: {run: "true"}\n' >.ironlint.yml
commit candidate-policy
denies
printf 'src/a.txt export-ignore\n' >.gitattributes
commit archive-omission
denies
printf 'allowed\n' >src/a.txt
commit restore
accepts
echo 'ok: old success is not reused; caller selects policy outside candidate'

printf '\n# policy changed\n' >>"$IRONLINT_POLICY"
expect 4 ironlint-fixture-harness change
denies
ironlint trust --config "$IRONLINT_POLICY" >/dev/null
accepts
echo 'ok: changed approved policy requires renewed consent'

cat >"$IRONLINT_POLICY" <<'YAML'
version: 1
execution:
  timeout_secs: 1
  total_timeout_secs: 2
checks:
  a_timeout:
    run: sleep 10
    on: [change, accept]
  z_later:
    run: touch must-not-run
    on: [change, accept]
YAML
export IRONLINT_ACCEPTANCE_CHECKS='a_timeout,z_later'
ironlint trust --config "$IRONLINT_POLICY" >/dev/null
expect 3 ironlint-fixture-harness change
verdict '.status == "error" and .results[0].reason == "timeout" and .not_run[0].id == "z_later"'
[[ ! -e must-not-run ]]
denies
echo 'ok: timeout stops remaining checks and denies acceptance'

printf 'version: 1\nchecks:\n  missing: {run: "ironlint-fixture-command-that-does-not-exist"}\n' >"$IRONLINT_POLICY"
export IRONLINT_ACCEPTANCE_CHECKS=missing
ironlint trust --config "$IRONLINT_POLICY" >/dev/null
denies
grep -q 'not_found' "$tmp/stderr"
expect 1 env IRONLINT_BIN=/missing/evaluator ironlint-fixture-harness accept
[[ ! -s "$tmp/stdout" ]]
echo 'ok: missing check executable and missing evaluator deny acceptance'

# Only the broken evaluator is a stub; preceding scenarios used the real binary.
printf '#!/bin/sh\nprintf "not-json\\n"\n' >"$tmp/broken-evaluator"
chmod +x "$tmp/broken-evaluator"
expect 1 env IRONLINT_BIN="$tmp/broken-evaluator" ironlint-fixture-harness accept
[[ ! -s "$tmp/stdout" ]]
cat >"$tmp/broken-evaluator" <<'SH'
#!/bin/sh
printf '%s\n' false '{"schema":7,"event":"accept","status":"pass","results":[{"id":"missing","outcome":"pass"}],"not_run":[],"error":null}'
SH
expect 1 env IRONLINT_BIN="$tmp/broken-evaluator" ironlint-fixture-harness accept
[[ ! -s "$tmp/stdout" ]]
echo 'ok: zero exit with malformed or multiple results does not authorize acceptance'
echo 'PASS: local IronLint feature E2E suite'
