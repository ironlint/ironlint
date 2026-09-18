#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir "$tmp/candidate"
git -C "$tmp/candidate" init -q
printf 'ignored.txt\n' >"$tmp/candidate/.gitignore"
printf '%s\n' 'export-ignored.txt export-ignore' 'export-substituted.txt export-subst' \
  >"$tmp/candidate/.gitattributes"
printf 'committed candidate\n' >"$tmp/candidate/tracked.txt"
printf 'must be evaluated\n' >"$tmp/candidate/export-ignored.txt"
printf '%s\n' 'candidate $Format:%H$' >"$tmp/candidate/export-substituted.txt"
mkdir "$tmp/candidate/nested"
printf 'tracked.txt export-ignore\n' >"$tmp/candidate/nested/.gitattributes"
printf 'nested candidate\n' >"$tmp/candidate/nested/tracked.txt"
git -C "$tmp/candidate" add .gitattributes .gitignore tracked.txt \
  export-ignored.txt export-substituted.txt nested
git -C "$tmp/candidate" -c user.email=ci@example.test -c user.name=CI commit -qm initial
sha="$(git -C "$tmp/candidate" rev-parse HEAD)"
printf 'live worktree only\n' >"$tmp/candidate/ignored.txt"

cat >"$tmp/docker" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$@" >"$IRONLINT_TEST_DOCKER_ARGS"
for arg in "$@"; do
  case "$arg" in
    type=bind,src=*,dst=/candidate,readonly) mount="$arg" ;;
  esac
done
candidate="${mount#type=bind,src=}"
candidate="${candidate%,dst=/candidate,readonly}"
[[ -f "$candidate/tracked.txt" \
  && -f "$candidate/export-ignored.txt" \
  && "$(<"$candidate/export-substituted.txt")" == 'candidate $Format:%H$' \
  && -f "$candidate/nested/tracked.txt" \
  && ! -e "$candidate/ignored.txt" ]] || {
  echo 'container did not receive an exact commit snapshot' >&2
  exit 1
}
printf '%s\n' "$IRONLINT_TEST_VERDICT"
EOF
chmod +x "$tmp/docker"

pass='{"schema":7,"event":"accept","status":"pass","results":[{"id":"clippy","outcome":"pass"},{"id":"fmt","outcome":"pass"},{"id":"test","outcome":"pass"}],"not_run":[],"error":null}'

run() {
  DOCKER_BIN="$tmp/docker" \
  IRONLINT_CONTAINER_IMAGE='ghcr.io/ironlint/acceptance@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef' \
  IRONLINT_ROOT="$tmp/candidate" \
  IRONLINT_CANDIDATE_SHA="$sha" \
  IRONLINT_ACCEPTANCE_CHECKS='clippy,fmt,test' \
  IRONLINT_TEST_DOCKER_ARGS="$tmp/docker-args" \
  IRONLINT_TEST_VERDICT="$pass" \
  "$root/scripts/run-containerized-acceptance.sh" >/dev/null 2>&1
}

run
args="$(<"$tmp/docker-args")"
for required in \
  '--network' 'none' '--read-only' '--cap-drop' 'ALL' \
  '--security-opt' 'no-new-privileges=true' '--pids-limit' '512' \
  'type=bind,src=' 'dst=/candidate,readonly' '--tmpfs' '/tmp:rw,nosuid,nodev,noexec' \
  '/work:rw,nosuid,nodev' '--user' '65532:65532' '/opt/ironlint/ironlint'; do
  [[ "$args" == *"$required"* ]] || { echo "missing docker argument: $required" >&2; exit 1; }
done

if IRONLINT_CONTAINER_IMAGE='ghcr.io/ironlint/acceptance:latest' \
  IRONLINT_ROOT="$tmp/candidate" \
  IRONLINT_CANDIDATE_SHA="$sha" \
  IRONLINT_ACCEPTANCE_CHECKS='clippy,fmt,test' \
  DOCKER_BIN="$tmp/docker" \
  "$root/scripts/run-containerized-acceptance.sh" >/dev/null 2>&1; then
  echo 'accepted mutable container image' >&2
  exit 1
fi

if IRONLINT_CONTAINER_IMAGE='ghcr.io/ironlint/acceptance@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef' \
  IRONLINT_ROOT="$tmp/candidate" \
  IRONLINT_CANDIDATE_SHA=not-the-candidate \
  IRONLINT_ACCEPTANCE_CHECKS='clippy,fmt,test' \
  DOCKER_BIN="$tmp/docker" \
  "$root/scripts/run-containerized-acceptance.sh" >/dev/null 2>&1; then
  echo 'accepted a mismatched candidate SHA' >&2
  exit 1
fi
