#!/usr/bin/env bash
# Test fixture only: models a caller, not a security boundary or real AI harness.
set -euo pipefail
: "${IRONLINT_POLICY:?}"
: "${IRONLINT_PROJECT:?}"
case "${1:-}" in
  change)
    shift
    exec ironlint check --config "$IRONLINT_POLICY" --root "$IRONLINT_PROJECT" \
      --event change --format json "$@"
    ;;
  accept)
    revision="$(git -C "$IRONLINT_PROJECT" rev-parse --verify HEAD^{commit})"
    snapshot="$(mktemp -d)"
    trap 'rm -rf "$snapshot"' EXIT
    git clone --quiet --no-local --no-checkout "$IRONLINT_PROJECT" "$snapshot"
    git -C "$snapshot" -c core.hooksPath=/dev/null checkout --quiet --detach "$revision"
    IRONLINT_ROOT="$snapshot" bash /opt/ironlint-tests/verify-acceptance.sh
    # The fixture's acceptance result identifies exactly the evaluated commit.
    printf '%s\n' "$revision"
    ;;
  *) echo 'usage: ironlint-fixture-harness change [--file PATH ...] | accept' >&2; exit 1 ;;
esac
