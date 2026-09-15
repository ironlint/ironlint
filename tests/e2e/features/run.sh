#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
image="ironlint-feature-e2e:$(date +%s)-$$"
trap 'docker image rm "$image" >/dev/null 2>&1 || true' EXIT
docker build -f "$root/tests/e2e/features/Dockerfile" -t "$image" "$root"
docker run --rm --network none --cap-drop ALL \
  --security-opt no-new-privileges --pids-limit 256 "$image"
