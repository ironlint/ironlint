#!/usr/bin/env bash
# Opt-in clean-room test for v1 init, consent, and Pi onboarding.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../../.." && pwd)"
IMAGE="ironlint-init-e2e:latest"
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
OUT="$HERE/runs/$RUN_ID"
HOME_DIR="$OUT/home"
PROJ_DIR="$OUT/project"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker not found on PATH — install Docker to run this test." >&2
  exit 127
fi

mkdir -p "$HOME_DIR" "$PROJ_DIR"
echo "run dir: $OUT"

echo "== building image =="
docker build -f "$HERE/Dockerfile" -t "$IMAGE" "$REPO_ROOT"

echo "== running container =="
docker run --rm \
  --user "$(id -u):$(id -g)" \
  -v "$HOME_DIR:/home/tester" \
  -v "$PROJ_DIR:/work" \
  -e HOME=/home/tester \
  -w /work \
  "$IMAGE" | tee "$OUT/container.log"

fail=0
pass() { printf '  ok   %s\n' "$1"; }
miss() { printf '  FAIL %s\n' "$1"; fail=1; }
exists() { if [ -e "$1" ]; then pass "$2"; else miss "$2 -> missing: $1"; fi; }
contains() {
  if [ -f "$1" ] && grep -qF -- "$2" "$1"; then pass "$3"; else miss "$3 -> grep '$2' in $1"; fi
}

echo "== assertions =="
exists "$PROJ_DIR/.pi/extensions/ironlint.ts" "Pi extension installed"
exists "$PROJ_DIR/.pi/extensions/.ironlint-adapter.json" "Pi extension ownership record"
exists "$PROJ_DIR/.pi/skills/ironlint-config/SKILL.md" "Pi authoring skill installed"
exists "$PROJ_DIR/.pi/skills/ironlint-config/.ironlint-adapter.json" "Pi skill ownership record"

exists "$PROJ_DIR/.ironlint.yml" "v1 policy scaffolded"
contains "$PROJ_DIR/.ironlint.yml" "version: 1" "scaffolded policy is v1"
exists "$HOME_DIR/.config/ironlint/trust.json" "local consent recorded"

exists "$PROJ_DIR/acceptance.json" "acceptance verdict captured"
contains "$PROJ_DIR/acceptance.json" '"schema": 7' "acceptance uses schema 7"
contains "$PROJ_DIR/acceptance.json" '"status": "pass"' "starter policy passes"

exists "$PROJ_DIR/doctor.json" "doctor report captured"
contains "$PROJ_DIR/doctor.json" '"name": "pi"' "doctor reports Pi"

echo
if [ "$fail" -eq 0 ]; then
  echo "PASS — v1 init and Pi onboarding assertions held ($OUT)"
else
  echo "FAIL — see failures above; forensics in $OUT"
fi
exit "$fail"
