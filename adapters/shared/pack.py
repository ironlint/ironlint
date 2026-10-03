#!/usr/bin/env python3
"""Build independent, self-contained plugin/installer archives from this repo."""
import argparse
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
HARNESSES = ("codex", "claude-code", "pi")


def package(harness, output, plugin_only=False):
    if plugin_only and harness == "pi":
        raise ValueError("Pi uses an installer/npm package, not a native plugin archive")
    source = ROOT / harness
    metadata = source / ("package.json" if harness == "pi" else "adapter.json")
    version = json.loads(metadata.read_text())["version"]
    output.mkdir(parents=True, exist_ok=True)
    kind = "-plugin" if plugin_only else ""
    archive = output / f"ironlint-{harness}{kind}-{version}.zip"
    files = {}
    for directory in ["hooks", "src", "bin", ".codex-plugin", ".claude-plugin"]:
        for path in (source / directory).rglob("*"):
            if path.is_file() and not path.is_symlink():
                files[f"{harness}/{path.relative_to(source)}"] = path
    for name in ["README.md", "completion.md", "completion-evidence.md", "adapter.json", "package.json"]:
        if (source / name).is_file():
            files[f"{harness}/{name}"] = source / name
    files[f"{harness}/LICENSE"] = ROOT.parent / "LICENSE"
    skill = ROOT / "shared/ironlint-config/SKILL.md"
    files["shared/ironlint-config/SKILL.md"] = skill
    files[f"{harness}/skills/ironlint-config/SKILL.md"] = skill
    if harness != "pi":
        for name in ["hook.py", "process.py"]:
            files[f"shared/hooks/{name}"] = ROOT / "shared/hooks" / name
            files[f"{harness}/hooks/{name}"] = ROOT / "shared/hooks" / name
        for path in (ROOT / "shared/completion").iterdir():
            if not path.is_file() or path.is_symlink() or (path.suffix != ".ts" and path.name != "package.json"):
                continue
            files[f"{harness}/shared/completion/{path.name}"] = path
        for name in ["candidate.ts", "acceptance.ts"]:
            files[f"{harness}/pi/src/{name}"] = ROOT / "pi/src" / name
        files[f"{harness}/pi/completion.md"] = ROOT / "pi/completion.md"
        if harness == "claude-code":
            for name in ["completion.md", "completion-evidence.md"]:
                if (ROOT / "codex" / name).is_file():
                    files[f"{harness}/codex/{name}"] = ROOT / "codex" / name
    if plugin_only:
        prefix = f"{harness}/"
        files = {name[len(prefix):]: path for name, path in files.items() if name.startswith(prefix)}
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        for name, path in sorted(files.items()):
            if path == source / "completion.md":
                # Source docs link sibling adapters; a standalone native
                # plugin carries those canonical contracts inside itself.
                content = path.read_text().replace("(../pi/completion.md", "(pi/completion.md")
                content = content.replace("(../codex/completion.md", "(codex/completion.md")
                bundle.writestr(zipfile.ZipInfo.from_file(path, name), content.encode())
            else:
                bundle.write(path, name)
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("harness", choices=HARNESSES)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--plugin-only", action="store_true", help="put native plugin content at the ZIP root")
    args = parser.parse_args()
    if args.plugin_only and args.harness == "pi":
        parser.error("--plugin-only requires codex or claude-code")
    print(package(args.harness, args.output, plugin_only=args.plugin_only))


if __name__ == "__main__":
    main()
