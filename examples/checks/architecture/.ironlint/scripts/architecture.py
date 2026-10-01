#!/usr/bin/env python3
"""Reject direct absolute imports from project.api into project.db (Python 3.11+)."""

import ast
from pathlib import Path
import sys

root = Path("src/project/api")
if not root.is_dir():
    print(f"{root}: missing API source directory; see docs/architecture.md")
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
            forbidden = any(alias.name == "project.db" or alias.name.startswith("project.db.") for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            module = node.module or ""
            forbidden = node.level == 0 and (
                module == "project.db" or module.startswith("project.db.")
                or (module == "project" and any(alias.name == "db" for alias in node.names))
            )
        else:
            continue
        if forbidden:
            print(f"{path}:{node.lineno}: project.api must use project.services instead of importing project.db; see docs/architecture.md")
            violations += 1

sys.exit(2 if violations else 0)
