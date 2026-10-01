#!/usr/bin/env python3
"""Keep the requests package out of project.core imports (Python 3.11+)."""

import ast
from pathlib import Path
import sys

root = Path("src/project/core")
if not root.is_dir():
    print(f"{root}: missing core source directory; see docs/dependencies.md")
    sys.exit(2)

violations = 0
for path in sorted(root.rglob("*.py")):
    try:
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    except (OSError, UnicodeError, SyntaxError) as error:
        print(f"{path}: cannot parse source: {error}")
        sys.exit(2)
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            forbidden = any(alias.name == "requests" or alias.name.startswith("requests.") for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            forbidden = node.level == 0 and (node.module == "requests" or (node.module or "").startswith("requests."))
        else:
            continue
        if forbidden:
            print(f"{path}:{node.lineno}: project.core must use project.adapters.http instead of requests; see docs/dependencies.md")
            violations += 1

sys.exit(2 if violations else 0)
