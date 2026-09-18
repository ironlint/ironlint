#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

: "${IRONLINT_CONTAINER_IMAGE:?set IRONLINT_CONTAINER_IMAGE to a digest-pinned image}"
: "${IRONLINT_ROOT:?set IRONLINT_ROOT to the candidate checkout}"
: "${IRONLINT_CANDIDATE_SHA:?set IRONLINT_CANDIDATE_SHA to the candidate commit}"
: "${IRONLINT_ACCEPTANCE_CHECKS:?set IRONLINT_ACCEPTANCE_CHECKS to the required IDs}"

if [[ ! "$IRONLINT_CONTAINER_IMAGE" =~ @sha256:[0-9a-f]{64}$ ]]; then
  echo 'IRONLINT_CONTAINER_IMAGE must be pinned by sha256 digest' >&2
  exit 1
fi

candidate_root="$(git -C "$IRONLINT_ROOT" rev-parse --show-toplevel)"
actual_sha="$(git -C "$candidate_root" rev-parse --verify 'HEAD^{commit}')"
if [[ "$actual_sha" != "$IRONLINT_CANDIDATE_SHA" ]]; then
  echo "candidate SHA mismatch: expected $IRONLINT_CANDIDATE_SHA, found $actual_sha" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
snapshot="$tmp/candidate"
mkdir "$snapshot"

while IFS=$'\t' read -r metadata path; do
  if [[ "$metadata" == 160000\ * ]]; then
    echo "candidate contains unsupported submodule: $path" >&2
    exit 1
  fi
done < <(git -C "$candidate_root" ls-tree -r --full-tree "$actual_sha")

# Export the commit so live, ignored, or concurrent worktree changes cannot
# alter the evaluated bytes. Acceptance checks intentionally receive no .git.
objects="$(git -C "$candidate_root" rev-parse --git-path objects)"
if [[ "$objects" != /* ]]; then
  objects="$candidate_root/$objects"
fi
objects="$(cd "$objects" && pwd -P)"
snapshot_git="$tmp/snapshot.git"
git init --bare -q "$snapshot_git"
printf '%s\n' "$objects" >"$snapshot_git/objects/info/alternates"
printf '%s\n' '* -export-ignore -export-subst' >"$snapshot_git/info/attributes"
git --git-dir="$snapshot_git" archive --format=tar "$actual_sha" | tar -xf - -C "$snapshot"

verdict="$tmp/verdict.json"
docker_bin="${DOCKER_BIN:-docker}"

if ! "$docker_bin" run --rm \
  --network none \
  --read-only \
  --cap-drop ALL \
  --security-opt no-new-privileges=true \
  --pids-limit 512 \
  --mount "type=bind,src=$snapshot,dst=/candidate,readonly" \
  --tmpfs /tmp:rw,nosuid,nodev,noexec \
  --tmpfs /work:rw,nosuid,nodev \
  --user 65532:65532 \
  --workdir /candidate \
  --env HOME=/work \
  --env TMPDIR=/tmp \
  --env CARGO_TARGET_DIR=/work/target \
  "$IRONLINT_CONTAINER_IMAGE" \
  /opt/ironlint/ironlint check \
  --config /opt/ironlint/policy.yml \
  --root /candidate \
  --event accept \
  --format json >"$verdict"; then
  cat "$verdict" >&2
  exit 1
fi

cat >"$tmp/replay-verdict" <<'EOF'
#!/usr/bin/env bash
cat "$IRONLINT_CONTAINER_VERDICT"
EOF
chmod +x "$tmp/replay-verdict"

IRONLINT_BIN="$tmp/replay-verdict" \
IRONLINT_POLICY=/opt/ironlint/policy.yml \
IRONLINT_ROOT=/candidate \
IRONLINT_ACCEPTANCE_CHECKS="$IRONLINT_ACCEPTANCE_CHECKS" \
IRONLINT_CONTAINER_VERDICT="$verdict" \
  "$root/scripts/verify-acceptance.sh"
