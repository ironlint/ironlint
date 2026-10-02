#!/bin/sh
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ -f "$HERE/hook.py" ]; then
  RUNNER="$HERE/hook.py"
else
  RUNNER="$HERE/../../shared/hooks/hook.py"
fi
exec python3 "$RUNNER" codex "$@"
