import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("pack", ROOT / "shared/pack.py")
pack = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pack)


class PackageTests(unittest.TestCase):
    def test_native_archives_include_explicit_completion_and_canonical_candidate(self):
        for harness, launcher in [("codex", "ironlint-codex-complete"),
                                  ("claude-code", "ironlint-claude-complete")]:
            with self.subTest(harness=harness), tempfile.TemporaryDirectory() as directory:
                archive = pack.package(harness, Path(directory), plugin_only=True)
                with zipfile.ZipFile(archive) as bundle:
                    names = bundle.namelist()
                    self.assertIn("bin/" + launcher, names)
                    self.assertIn("shared/completion/cli.ts", names)
                    self.assertIn("shared/completion/completion.ts", names)
                    self.assertIn("completion-evidence.md", names)
                    for module in ["candidate.ts", "acceptance.ts"]:
                        self.assertEqual(bundle.read("pi/src/" + module),
                                         (ROOT / "pi/src" / module).read_bytes())
                    self.assertFalse(any("/test/" in name for name in names))
                    bundle.extractall(Path(directory) / "cached")
                cached = Path(directory) / "cached"
                for document in [cached / "completion.md", *cached.glob("codex/completion.md")]:
                    for target in re.findall(r"\[[^\]]+\]\(([^)]+)\)", document.read_text()):
                        if "://" not in target and not target.startswith("#"):
                            self.assertTrue((document.parent / target.split("#")[0]).is_file(), target)
                child = subprocess.run(["sh", str(Path(directory) / "cached/bin" / launcher)],
                                       capture_output=True, timeout=10)
                self.assertEqual(child.returncode, 3, child.stderr)
                output = json.loads(child.stdout)
                self.assertEqual(output["status"], "incomplete")
                self.assertNotIn("candidate", output)

    def test_each_archive_installs_without_the_source_checkout(self):
        for harness in pack.HARNESSES:
            with self.subTest(harness=harness), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                archive = pack.package(harness, root)
                with zipfile.ZipFile(archive) as bundle:
                    self.assertTrue(any(name.endswith("SKILL.md") for name in bundle.namelist()))
                    self.assertFalse(any("node_modules" in name or "/test/" in name for name in bundle.namelist()))
                    bundle.extractall(root / "unpacked")
                if harness == "pi":
                    self.assertTrue((root / "unpacked/pi/src/index.ts").is_file())
                    continue
                plugin = root / "unpacked" / harness
                # Simulate a plugin manager copying only this plugin directory.
                shutil.rmtree(root / "unpacked/shared")
                payload = json.dumps({"hook_event_name": "Stop", "cwd": str(root)}).encode()
                child = subprocess.run(["sh", str(plugin / "hooks/hook.sh"), "stop"], cwd=root, input=payload, capture_output=True)
                self.assertEqual(child.returncode, 0, child.stderr)
                self.assertIn("incomplete", json.loads(child.stdout)["systemMessage"])

    def test_marketplace_plugin_runs_after_cache_copy(self):
        catalog = json.loads((ROOT.parent / ".claude-plugin/marketplace.json").read_text())
        entry = catalog["plugins"][0]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = entry["source"]
            if isinstance(source, str):
                plugin = ROOT.parent / source
            else:
                self.assertEqual(source["source"], "archive")
                archive = pack.package("claude-code", root, plugin_only=True)
                self.assertTrue(source["url"].startswith("https://"))
                # Publication/catalog updates are separately authorized. Build
                # and exercise the current local package without repinning the
                # catalog's previously prepared versioned release URL.
                self.assertTrue(source["url"].endswith(".zip"))
                with zipfile.ZipFile(archive) as bundle:
                    bundle.extractall(root / "unpacked")
                plugin = root / "unpacked"
                if not (plugin / ".claude-plugin/plugin.json").is_file():
                    entries = list(plugin.iterdir())
                    self.assertEqual(len(entries), 1, "archive must have plugin content at root or inside one wrapper")
                    plugin = entries[0]
            cached = root / "cache/plugin"
            shutil.copytree(plugin, cached)
            for mode, event in [("post-tool-use", "PostToolUse"), ("stop", "Stop")]:
                payload = json.dumps({"hook_event_name": event, "cwd": str(root)}).encode()
                child = subprocess.run(
                    ["sh", str(cached / "hooks/hook.sh"), mode],
                    cwd=root, input=payload, capture_output=True, timeout=10,
                )
                self.assertEqual(child.returncode, 0, child.stderr)
                output = json.loads(child.stdout)
                if mode == "stop":
                    self.assertIn("incomplete", output["systemMessage"])
                else:
                    self.assertEqual(output, {})
            manifest = json.loads((cached / ".claude-plugin/plugin.json").read_text())
            self.assertEqual(entry["name"], manifest["name"])
            self.assertTrue((cached / "skills/ironlint-config/SKILL.md").is_file())

    def test_native_plugin_archives_have_root_manifests_and_all_dependencies(self):
        for harness in ["codex", "claude-code"]:
            with self.subTest(harness=harness), tempfile.TemporaryDirectory() as directory:
                archive = pack.package(harness, Path(directory), plugin_only=True)
                manifest = ".codex-plugin/plugin.json" if harness == "codex" else ".claude-plugin/plugin.json"
                with zipfile.ZipFile(archive) as bundle:
                    names = bundle.namelist()
                    for name in [manifest, "hooks/hooks.json", "hooks/hook.sh", "hooks/hook.py", "hooks/process.py", "skills/ironlint-config/SKILL.md", "LICENSE"]:
                        self.assertIn(name, names)
                    self.assertFalse(any(name.startswith(harness + "/") for name in names))
                    self.assertTrue(all(not name.startswith("shared/") or name.startswith("shared/completion/")
                                        for name in names))
        with tempfile.TemporaryDirectory() as directory, self.assertRaises(ValueError):
            pack.package("pi", Path(directory), plugin_only=True)


if __name__ == "__main__":
    unittest.main()
