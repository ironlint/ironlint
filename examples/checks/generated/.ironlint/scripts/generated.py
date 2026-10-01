#!/usr/bin/env python3
"""Check that generated/catalog.json is canonical JSON from data/catalog.json."""

import json
from pathlib import Path
import sys

source = Path("data/catalog.json")
generated = Path("generated/catalog.json")
try:
    value = json.loads(source.read_text(encoding="utf-8"))
    actual = generated.read_text(encoding="utf-8")
except (OSError, UnicodeError, json.JSONDecodeError) as error:
    print(f"{source} or {generated}: missing or invalid JSON: {error}")
    sys.exit(2)

expected = json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
if actual != expected:
    print(f"{generated}: differs from canonical {source}; sort keys, indent by two spaces, and end with a newline; see docs/generated.md")
    sys.exit(2)
